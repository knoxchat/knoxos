/// Clipboard Manager — Cross-application copy/paste with MIME type support
///
/// Implements a centralized clipboard service for the KnoxOS desktop:
///   - Multiple MIME types per clipboard entry (text/plain, text/html, image/png, etc.)
///   - Primary selection (middle-click paste) and clipboard selection (Ctrl+C/V)
///   - Clipboard history (last 32 entries)
///   - Serialization for inter-window data transfer
///   - Clipboard viewer/manager UI integration
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use super::window::WindowId;

// ═══════════════════════════════════════════════════════════════════════
// MIME TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Well-known MIME types for clipboard content
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MimeType {
    /// UTF-8 plain text
    TextPlain,
    /// HTML formatted text
    TextHtml,
    /// URI list (file paths, URLs)
    TextUriList,
    /// PNG image data
    ImagePng,
    /// BGRA raw pixel data (internal format)
    ImageBgra,
    /// Application-specific data
    ApplicationOctetStream,
}

impl MimeType {
    /// Return the MIME type string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            MimeType::TextPlain => "text/plain;charset=utf-8",
            MimeType::TextHtml => "text/html",
            MimeType::TextUriList => "text/uri-list",
            MimeType::ImagePng => "image/png",
            MimeType::ImageBgra => "image/x-knoxos-bgra",
            MimeType::ApplicationOctetStream => "application/octet-stream",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CLIPBOARD ENTRY
// ═══════════════════════════════════════════════════════════════════════

/// A single clipboard entry can hold data in multiple MIME formats.
/// When the user copies rich text, both text/plain and text/html may be stored.
#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    /// MIME type → data mapping. Multiple representations of the same content.
    pub data: BTreeMap<MimeType, Vec<u8>>,
    /// Source window that created this entry (if any)
    pub source: Option<WindowId>,
    /// Monotonic sequence number for ordering
    pub sequence: u64,
    /// Timestamp (TSC ticks) when entry was created
    pub timestamp: u64,
}

impl ClipboardEntry {
    /// Create a new clipboard entry from plain text
    pub fn from_text(text: &str, source: Option<WindowId>, seq: u64, ts: u64) -> Self {
        let mut data = BTreeMap::new();
        data.insert(MimeType::TextPlain, text.as_bytes().to_vec());
        Self {
            data,
            source,
            sequence: seq,
            timestamp: ts,
        }
    }

    /// Create a new clipboard entry from HTML (stores both HTML and plain text)
    pub fn from_html(html: &str, plain: &str, source: Option<WindowId>, seq: u64, ts: u64) -> Self {
        let mut data = BTreeMap::new();
        data.insert(MimeType::TextHtml, html.as_bytes().to_vec());
        data.insert(MimeType::TextPlain, plain.as_bytes().to_vec());
        Self {
            data,
            source,
            sequence: seq,
            timestamp: ts,
        }
    }

    /// Create a new clipboard entry from raw image data
    pub fn from_image(
        bgra_data: &[u8],
        width: u32,
        height: u32,
        source: Option<WindowId>,
        seq: u64,
        ts: u64,
    ) -> Self {
        let mut data = BTreeMap::new();
        // Store dimensions in header: 4 bytes width + 4 bytes height + pixel data
        let mut buf = Vec::with_capacity(8 + bgra_data.len());
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&height.to_le_bytes());
        buf.extend_from_slice(bgra_data);
        data.insert(MimeType::ImageBgra, buf);
        Self {
            data,
            source,
            sequence: seq,
            timestamp: ts,
        }
    }

    /// Create a clipboard entry from a URI list (file paths, URLs)
    pub fn from_uris(uris: &[&str], source: Option<WindowId>, seq: u64, ts: u64) -> Self {
        let mut data = BTreeMap::new();
        let uri_text = uris.join("\r\n");
        data.insert(MimeType::TextUriList, uri_text.as_bytes().to_vec());
        // Also provide as plain text
        data.insert(MimeType::TextPlain, uri_text.as_bytes().to_vec());
        Self {
            data,
            source,
            sequence: seq,
            timestamp: ts,
        }
    }

    /// Get content as plain text (best effort conversion)
    pub fn as_text(&self) -> Option<&[u8]> {
        self.data
            .get(&MimeType::TextPlain)
            .or_else(|| self.data.get(&MimeType::TextUriList))
            .map(|v| v.as_slice())
    }

    /// Get content for a specific MIME type
    pub fn get(&self, mime: MimeType) -> Option<&[u8]> {
        self.data.get(&mime).map(|v| v.as_slice())
    }

    /// Get the list of available MIME types
    pub fn available_types(&self) -> Vec<MimeType> {
        self.data.keys().copied().collect()
    }

    /// Add an additional MIME representation
    pub fn add_representation(&mut self, mime: MimeType, data: Vec<u8>) {
        self.data.insert(mime, data);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SELECTION TYPE
// ═══════════════════════════════════════════════════════════════════════

/// X11/Wayland-style selection types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// Standard clipboard (Ctrl+C / Ctrl+V)
    Clipboard,
    /// Primary selection (selected text, middle-click paste)
    Primary,
}

// ═══════════════════════════════════════════════════════════════════════
// CLIPBOARD STATE
// ═══════════════════════════════════════════════════════════════════════

const MAX_HISTORY: usize = 32;
const MAX_ENTRY_SIZE: usize = 16 * 1024 * 1024; // 16 MB per entry

struct ClipboardState {
    /// Current clipboard content
    clipboard: Option<ClipboardEntry>,
    /// Current primary selection
    primary: Option<ClipboardEntry>,
    /// Clipboard history (most recent first)
    history: Vec<ClipboardEntry>,
    /// Monotonic sequence counter
    next_seq: u64,
    /// Whether the clipboard viewer is open
    viewer_open: bool,
    /// Clipboard viewer scroll offset
    viewer_scroll: i32,
    /// Selected history item in viewer
    viewer_selected: Option<usize>,
}

impl ClipboardState {
    fn new() -> Self {
        Self {
            clipboard: None,
            primary: None,
            history: Vec::new(),
            next_seq: 1,
            viewer_open: false,
            viewer_scroll: 0,
            viewer_selected: None,
        }
    }

    fn next_sequence(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<ClipboardState> = Mutex::new(ClipboardState::new());
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Copy text to the clipboard
pub fn copy_text(text: &str, source: Option<WindowId>) {
    if text.is_empty() || text.len() > MAX_ENTRY_SIZE {
        return;
    }
    let mut state = STATE.lock();
    let seq = state.next_sequence();
    let ts = crate::gui::read_tsc_public();
    let entry = ClipboardEntry::from_text(text, source, seq, ts);
    // Add to history
    state.history.insert(0, entry.clone());
    if state.history.len() > MAX_HISTORY {
        state.history.pop();
    }
    state.clipboard = Some(entry);
}

/// Copy HTML content (with plain text fallback) to the clipboard
pub fn copy_html(html: &str, plain: &str, source: Option<WindowId>) {
    if html.is_empty() || html.len() > MAX_ENTRY_SIZE {
        return;
    }
    let mut state = STATE.lock();
    let seq = state.next_sequence();
    let ts = crate::gui::read_tsc_public();
    let entry = ClipboardEntry::from_html(html, plain, source, seq, ts);
    state.history.insert(0, entry.clone());
    if state.history.len() > MAX_HISTORY {
        state.history.pop();
    }
    state.clipboard = Some(entry);
}

/// Copy image data to the clipboard
pub fn copy_image(bgra_data: &[u8], width: u32, height: u32, source: Option<WindowId>) {
    if bgra_data.is_empty() || bgra_data.len() > MAX_ENTRY_SIZE {
        return;
    }
    let mut state = STATE.lock();
    let seq = state.next_sequence();
    let ts = crate::gui::read_tsc_public();
    let entry = ClipboardEntry::from_image(bgra_data, width, height, source, seq, ts);
    state.history.insert(0, entry.clone());
    if state.history.len() > MAX_HISTORY {
        state.history.pop();
    }
    state.clipboard = Some(entry);
}

/// Copy file URIs to the clipboard (for file manager copy/paste)
pub fn copy_uris(uris: &[&str], source: Option<WindowId>) {
    let mut state = STATE.lock();
    let seq = state.next_sequence();
    let ts = crate::gui::read_tsc_public();
    let entry = ClipboardEntry::from_uris(uris, source, seq, ts);
    state.history.insert(0, entry.clone());
    if state.history.len() > MAX_HISTORY {
        state.history.pop();
    }
    state.clipboard = Some(entry);
}

/// Set the primary selection (highlighted text)
pub fn set_primary(text: &str, source: Option<WindowId>) {
    if text.is_empty() || text.len() > MAX_ENTRY_SIZE {
        return;
    }
    let mut state = STATE.lock();
    let seq = state.next_sequence();
    let ts = crate::gui::read_tsc_public();
    state.primary = Some(ClipboardEntry::from_text(text, source, seq, ts));
}

/// Paste from clipboard — returns the entry for the requested MIME type
pub fn paste(selection: Selection, preferred_mime: MimeType) -> Option<Vec<u8>> {
    let state = STATE.lock();
    let entry = match selection {
        Selection::Clipboard => state.clipboard.as_ref(),
        Selection::Primary => state.primary.as_ref(),
    };
    entry.and_then(|e| {
        e.get(preferred_mime)
            .or_else(|| e.as_text())
            .map(|d| d.to_vec())
    })
}

/// Paste plain text from clipboard (convenience)
pub fn paste_text(selection: Selection) -> Option<String> {
    paste(selection, MimeType::TextPlain)
        .and_then(|bytes| core::str::from_utf8(&bytes).ok().map(|s| String::from(s)))
}

/// Get clipboard entry reference (for type inspection)
pub fn get_entry(selection: Selection) -> Option<ClipboardEntry> {
    let state = STATE.lock();
    match selection {
        Selection::Clipboard => state.clipboard.clone(),
        Selection::Primary => state.primary.clone(),
    }
}

/// Check if the clipboard contains data of a specific type
pub fn has_type(selection: Selection, mime: MimeType) -> bool {
    let state = STATE.lock();
    let entry = match selection {
        Selection::Clipboard => state.clipboard.as_ref(),
        Selection::Primary => state.primary.as_ref(),
    };
    entry.map_or(false, |e| e.data.contains_key(&mime))
}

/// Get available MIME types in the current clipboard
pub fn available_types(selection: Selection) -> Vec<MimeType> {
    let state = STATE.lock();
    let entry = match selection {
        Selection::Clipboard => state.clipboard.as_ref(),
        Selection::Primary => state.primary.as_ref(),
    };
    entry.map_or_else(Vec::new, |e| e.available_types())
}

/// Clear the clipboard
pub fn clear(selection: Selection) {
    let mut state = STATE.lock();
    match selection {
        Selection::Clipboard => state.clipboard = None,
        Selection::Primary => state.primary = None,
    }
}

/// Clear the history
pub fn clear_history() {
    STATE.lock().history.clear();
}

/// Get clipboard history entries (for clipboard viewer)
pub fn history() -> Vec<ClipboardEntry> {
    STATE.lock().history.clone()
}

/// Paste from history by index
pub fn paste_from_history(index: usize) -> Option<ClipboardEntry> {
    let mut state = STATE.lock();
    if let Some(entry) = state.history.get(index).cloned() {
        state.clipboard = Some(entry.clone());
        Some(entry)
    } else {
        None
    }
}

/// Delete a specific history entry
pub fn delete_history(index: usize) {
    let mut state = STATE.lock();
    if index < state.history.len() {
        state.history.remove(index);
    }
}

/// Toggle the clipboard viewer UI
pub fn toggle_viewer() {
    let mut state = STATE.lock();
    state.viewer_open = !state.viewer_open;
    if state.viewer_open {
        state.viewer_scroll = 0;
        state.viewer_selected = None;
    }
    crate::gui::request_redraw();
}

/// Check if clipboard viewer is open
pub fn is_viewer_open() -> bool {
    STATE.lock().viewer_open
}

// ═══════════════════════════════════════════════════════════════════════
// CLIPBOARD VIEWER UI
// ═══════════════════════════════════════════════════════════════════════

const VIEWER_WIDTH: u32 = 380;
const VIEWER_MAX_HEIGHT: u32 = 500;
const VIEWER_ITEM_HEIGHT: u32 = 52;
const VIEWER_PADDING: u32 = 12;
const VIEWER_RADIUS: u32 = 12;

/// Draw the clipboard viewer popup (called from desktop overlay layer)
pub fn draw_viewer(fb: &mut FrameBuffer) {
    let state = STATE.lock();
    if !state.viewer_open {
        return;
    }

    let colors = theme::colors();
    let (sw, sh) = crate::gui::cached_screen_size();

    // Position: center-right, above taskbar
    let vx = sw - VIEWER_WIDTH as i32 - 20;
    let item_count = state.history.len().min(8); // show up to 8
    let content_h = (item_count as u32 * VIEWER_ITEM_HEIGHT) + VIEWER_PADDING * 2 + 36;
    let vh = content_h.min(VIEWER_MAX_HEIGHT);
    let vy = sh - 70 - vh as i32;

    let panel = Rect::new(vx, vy, VIEWER_WIDTH, vh);

    // Background with blur effect
    fb.fill_rounded_rect_aa(panel, colors.bg_elevated.with_alpha(240), VIEWER_RADIUS);

    // Title bar
    let title_y = vy + VIEWER_PADDING as i32;
    fonts::draw_string_compact(
        fb,
        vx + VIEWER_PADDING as i32,
        title_y,
        "Clipboard",
        colors.text_primary,
        1,
    );

    // Clear button
    let clear_x = vx + VIEWER_WIDTH as i32 - 60;
    fonts::draw_string_compact(fb, clear_x, title_y, "Clear", colors.accent_primary, 1);

    // Items
    let items_y = title_y + 28;
    for (i, entry) in state.history.iter().enumerate().take(8) {
        let iy = items_y + (i as i32 * VIEWER_ITEM_HEIGHT as i32) - state.viewer_scroll;
        if iy + VIEWER_ITEM_HEIGHT as i32 <= vy || iy >= vy + vh as i32 {
            continue;
        }

        let item_rect = Rect::new(vx + 8, iy, VIEWER_WIDTH - 16, VIEWER_ITEM_HEIGHT - 4);
        let is_selected = state.viewer_selected == Some(i);

        // Item background
        let item_bg = if is_selected {
            colors.selected_bg
        } else {
            colors.bg_surface
        };
        fb.fill_rounded_rect_aa(item_rect, item_bg, 8);

        // MIME type badge
        let mime_str = if entry.data.contains_key(&MimeType::ImageBgra)
            || entry.data.contains_key(&MimeType::ImagePng)
        {
            "IMG"
        } else if entry.data.contains_key(&MimeType::TextHtml) {
            "HTML"
        } else if entry.data.contains_key(&MimeType::TextUriList) {
            "URI"
        } else {
            "TXT"
        };
        fonts::draw_string_compact(fb, item_rect.x + 8, iy + 6, mime_str, colors.accent_info, 1);

        // Preview text
        if let Some(text_bytes) = entry.as_text() {
            if let Ok(text) = core::str::from_utf8(text_bytes) {
                // Truncate to first line, max 40 chars
                let preview: String = text
                    .chars()
                    .take(40)
                    .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                    .collect();
                fonts::draw_string_compact(
                    fb,
                    item_rect.x + 44,
                    iy + 6,
                    &preview,
                    colors.text_primary,
                    1,
                );
            }
        }

        // Size info
        let total_size: usize = entry.data.values().map(|v| v.len()).sum();
        let size_str = if total_size < 1024 {
            alloc::format!("{} B", total_size)
        } else if total_size < 1024 * 1024 {
            alloc::format!("{} KB", total_size / 1024)
        } else {
            alloc::format!("{} MB", total_size / (1024 * 1024))
        };
        fonts::draw_string_compact(
            fb,
            item_rect.x + 8,
            iy + 28,
            &size_str,
            colors.text_muted,
            1,
        );
    }

    // Empty state
    if state.history.is_empty() {
        let empty_y = items_y + 20;
        fonts::draw_string_compact(
            fb,
            vx + 40,
            empty_y,
            "No clipboard history",
            colors.text_muted,
            1,
        );
    }
}

/// Handle click inside clipboard viewer
pub fn handle_viewer_click(x: i32, y: i32) -> bool {
    let mut state = STATE.lock();
    if !state.viewer_open {
        return false;
    }

    let (sw, sh) = crate::gui::cached_screen_size();
    let vx = sw - VIEWER_WIDTH as i32 - 20;
    let item_count = state.history.len().min(8);
    let content_h = (item_count as u32 * VIEWER_ITEM_HEIGHT) + VIEWER_PADDING * 2 + 36;
    let vh = content_h.min(VIEWER_MAX_HEIGHT);
    let vy = sh - 70 - vh as i32;

    // Check bounds
    if x < vx || x > vx + VIEWER_WIDTH as i32 || y < vy || y > vy + vh as i32 {
        state.viewer_open = false;
        crate::gui::request_redraw();
        return true;
    }

    // Clear button
    let clear_x = vx + VIEWER_WIDTH as i32 - 60;
    let title_y = vy + VIEWER_PADDING as i32;
    if y >= title_y && y < title_y + 20 && x >= clear_x {
        state.history.clear();
        state.clipboard = None;
        crate::gui::request_redraw();
        return true;
    }

    // Item click — paste and close
    let items_y = title_y + 28;
    for i in 0..item_count {
        let iy = items_y + (i as i32 * VIEWER_ITEM_HEIGHT as i32) - state.viewer_scroll;
        if y >= iy && y < iy + VIEWER_ITEM_HEIGHT as i32 {
            if let Some(entry) = state.history.get(i).cloned() {
                state.clipboard = Some(entry);
                state.viewer_open = false;
                crate::gui::request_redraw();
            }
            return true;
        }
    }

    true
}

/// Handle scroll inside clipboard viewer
pub fn handle_viewer_scroll(delta: i32) -> bool {
    let mut state = STATE.lock();
    if !state.viewer_open {
        return false;
    }
    state.viewer_scroll = (state.viewer_scroll - delta * 20).max(0);
    let max_scroll = (state.history.len() as i32 * VIEWER_ITEM_HEIGHT as i32)
        .saturating_sub(VIEWER_MAX_HEIGHT as i32);
    state.viewer_scroll = state.viewer_scroll.min(max_scroll.max(0));
    crate::gui::request_redraw();
    true
}
