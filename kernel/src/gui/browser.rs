/// Browser Engine — Interactive Vivaldi/KnoxOS browser with URL navigation,
/// clickable speed dial, page loading, history, and keyboard input.
///
/// This module manages per-window browser state and provides:
/// - URL bar editing with cursor, selection, and navigation
/// - Speed dial tiles that navigate on click
/// - Page loading via the kernel HTTP client
/// - Back/forward navigation history
/// - Tab management (new tab, close tab)
/// - Rendered page content from HTML responses
/// - Keyboard shortcuts (Ctrl+L focus URL bar, Enter navigate, etc.)
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;

const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use spin::Mutex;

use super::window::WindowId;

// ═══════════════════════════════════════════════════════════════════════
// BOOKMARK STORAGE (9.80)
// ═══════════════════════════════════════════════════════════════════════

/// A saved bookmark entry
#[derive(Debug, Clone)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

lazy_static::lazy_static! {
    /// Global bookmarks list
    pub static ref BOOKMARKS: Mutex<Vec<Bookmark>> = Mutex::new(vec![
        Bookmark { title: String::from("KnoxOS Home"), url: String::from("knoxos://home") },
        Bookmark { title: String::from("GitHub"), url: String::from("https://github.com") },
        Bookmark { title: String::from("Wikipedia"), url: String::from("https://en.wikipedia.org") },
        Bookmark { title: String::from("Vivaldi"), url: String::from("https://vivaldi.com") },
        Bookmark { title: String::from("Reddit"), url: String::from("https://www.reddit.com") },
        Bookmark { title: String::from("YouTube"), url: String::from("https://www.youtube.com") },
    ]);
}

/// Add a bookmark for the given URL and title
pub fn add_bookmark(title: &str, url: &str) {
    let mut bm = BOOKMARKS.lock();
    // Don't duplicate
    if bm.iter().any(|b| b.url == url) {
        return;
    }
    bm.push(Bookmark {
        title: String::from(title),
        url: String::from(url),
    });
}

/// Remove a bookmark by URL
pub fn remove_bookmark(url: &str) {
    let mut bm = BOOKMARKS.lock();
    bm.retain(|b| b.url != url);
}

/// Check if a URL is bookmarked
pub fn is_bookmarked(url: &str) -> bool {
    let bm = BOOKMARKS.lock();
    bm.iter().any(|b| b.url == url)
}

/// Toggle bookmark for the current page
pub fn toggle_bookmark(title: &str, url: &str) {
    if is_bookmarked(url) {
        remove_bookmark(url);
    } else {
        add_bookmark(title, url);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BROWSER STATE
// ═══════════════════════════════════════════════════════════════════════

/// Which UI element is focused in the browser window
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserFocus {
    /// No element focused (page content area)
    Page,
    /// URL bar is focused and editable
    UrlBar,
    /// Search bar on speed dial page
    SearchBar,
}

/// A page that has been loaded (or a built-in page)
#[derive(Debug, Clone)]
pub struct LoadedPage {
    /// The URL this page was loaded from
    pub url: String,
    /// Page title extracted from content
    pub title: String,
    /// Plain text lines to render (extracted from HTML or built-in)
    pub lines: Vec<PageLine>,
    /// Whether this is a built-in page (speed dial, error, etc.)
    pub builtin: bool,
    /// HTTP status code (0 for built-in pages)
    pub status_code: u16,
    /// Whether page is still loading
    pub loading: bool,
}

/// A line of rendered page content with optional styling
#[derive(Debug, Clone)]
pub struct PageLine {
    pub text: String,
    pub style: LineStyle,
    /// Clickable link target (if this line is a link)
    pub link_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    /// Large heading (h1)
    Heading1,
    /// Medium heading (h2)
    Heading2,
    /// Small heading (h3)
    Heading3,
    /// Normal paragraph text
    Paragraph,
    /// Bulleted list item
    ListItem,
    /// Clickable hyperlink
    Link,
    /// Blank separator line
    Blank,
    /// Preformatted / code text
    Code,
    /// Error message
    Error,
}

/// Per-window browser instance state
#[derive(Debug, Clone)]
pub struct BrowserState {
    /// Current URL in the URL bar (may differ from loaded page during editing)
    pub url_text: String,
    /// Cursor position in URL bar (byte offset)
    pub url_cursor: usize,
    /// Which element is focused
    pub focus: BrowserFocus,
    /// Search bar text (on speed dial page)
    pub search_text: String,
    /// Search bar cursor position
    pub search_cursor: usize,
    /// Currently loaded page
    pub page: LoadedPage,
    /// Navigation history (back stack)
    pub back_stack: Vec<String>,
    /// Forward stack (populated when going back)
    pub forward_stack: Vec<String>,
    /// Whether this is a Vivaldi-branded browser
    pub vivaldi: bool,
    /// Hovered link index (for mouse hover on links)
    pub hovered_link: Option<usize>,
    /// Status bar text (shown at bottom, e.g., link URL on hover)
    pub status_text: String,
}

impl BrowserState {
    pub fn new(vivaldi: bool) -> Self {
        let start_url = if vivaldi {
            String::from("vivaldi://newtab")
        } else {
            String::from("knoxos://home")
        };
        let page = if vivaldi {
            make_speed_dial_page()
        } else {
            make_knoxos_home_page()
        };
        Self {
            url_text: start_url,
            url_cursor: 0,
            focus: BrowserFocus::Page,
            search_text: String::new(),
            search_cursor: 0,
            page,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            vivaldi,
            hovered_link: None,
            status_text: String::new(),
        }
    }

    /// Navigate to a URL — pushes current page to back stack, loads new page
    pub fn navigate(&mut self, url: &str) {
        // Don't navigate to same URL
        if url == self.page.url && !self.page.loading {
            return;
        }

        // Push current page URL to back stack
        if !self.page.url.is_empty() {
            self.back_stack.push(self.page.url.clone());
        }
        // Clear forward stack on new navigation
        self.forward_stack.clear();

        // Load the page
        self.load_url(url);
    }

    /// Go back in history
    pub fn go_back(&mut self) {
        if let Some(prev_url) = self.back_stack.pop() {
            self.forward_stack.push(self.page.url.clone());
            self.load_url(&prev_url);
        }
    }

    /// Go forward in history
    pub fn go_forward(&mut self) {
        if let Some(next_url) = self.forward_stack.pop() {
            self.back_stack.push(self.page.url.clone());
            self.load_url(&next_url);
        }
    }

    /// Reload current page
    pub fn reload(&mut self) {
        let url = self.page.url.clone();
        self.load_url(&url);
    }

    /// Internal: load a URL and set page content
    fn load_url(&mut self, url: &str) {
        self.url_text = String::from(url);
        self.url_cursor = self.url_text.len();
        self.focus = BrowserFocus::Page;
        self.hovered_link = None;
        self.status_text.clear();

        // Handle built-in URLs
        if url == "vivaldi://newtab" || url == "vivaldi://speeddial" {
            self.page = make_speed_dial_page();
            return;
        }
        if url == "knoxos://home" || url == "about:blank" {
            self.page = make_knoxos_home_page();
            return;
        }
        if url == "vivaldi://settings" || url == "chrome://settings" {
            self.page = make_settings_page();
            return;
        }
        if url == "vivaldi://about" || url == "chrome://about" {
            self.page = make_about_page(self.vivaldi);
            return;
        }
        if url == "vivaldi://history" || url == "chrome://history" {
            self.page = make_history_page(&self.back_stack);
            return;
        }
        if url == "vivaldi://bookmarks" || url == "chrome://bookmarks" {
            self.page = make_bookmarks_page();
            return;
        }

        // Try to load via HTTP
        self.page = load_http_page(url);
    }

    // ─── URL bar editing ─────────────────────────────────────────────

    /// Handle a character typed into the URL bar
    pub fn url_bar_char(&mut self, ch: char) {
        self.url_text.insert(self.url_cursor, ch);
        self.url_cursor += ch.len_utf8();
    }

    /// Handle backspace in URL bar
    pub fn url_bar_backspace(&mut self) {
        if self.url_cursor > 0 {
            // Find the previous character boundary
            let prev = self.url_text[..self.url_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.url_text.drain(prev..self.url_cursor);
            self.url_cursor = prev;
        }
    }

    /// Handle delete in URL bar
    pub fn url_bar_delete(&mut self) {
        if self.url_cursor < self.url_text.len() {
            let next = self.url_text[self.url_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.url_cursor + i)
                .unwrap_or(self.url_text.len());
            self.url_text.drain(self.url_cursor..next);
        }
    }

    /// Move URL bar cursor left
    pub fn url_bar_left(&mut self) {
        if self.url_cursor > 0 {
            self.url_cursor = self.url_text[..self.url_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move URL bar cursor right
    pub fn url_bar_right(&mut self) {
        if self.url_cursor < self.url_text.len() {
            self.url_cursor = self.url_text[self.url_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.url_cursor + i)
                .unwrap_or(self.url_text.len());
        }
    }

    /// Move URL bar cursor to start
    pub fn url_bar_home(&mut self) {
        self.url_cursor = 0;
    }

    /// Move URL bar cursor to end
    pub fn url_bar_end(&mut self) {
        self.url_cursor = self.url_text.len();
    }

    /// Select all text in URL bar
    pub fn url_bar_select_all(&mut self) {
        self.url_cursor = self.url_text.len();
    }

    /// Clear URL bar and focus it
    pub fn focus_url_bar(&mut self) {
        self.focus = BrowserFocus::UrlBar;
        self.url_cursor = self.url_text.len();
    }

    /// Submit URL bar (Enter key) — navigate to the entered URL
    pub fn url_bar_submit(&mut self) {
        let url = self.url_text.trim().to_string();
        if url.is_empty() {
            return;
        }

        // If it looks like a search query (no dots, no scheme), wrap in search URL
        let navigable_url = if !url.contains('.') && !url.contains("://") && !url.contains(':') {
            format!("https://www.google.com/search?q={}", url.replace(' ', "+"))
        } else if !url.contains("://") {
            format!("https://{}", url)
        } else {
            url.clone()
        };

        self.focus = BrowserFocus::Page;
        self.navigate(&navigable_url);
    }

    // ─── Search bar editing (speed dial) ────────────────────────────

    pub fn search_bar_char(&mut self, ch: char) {
        self.search_text.insert(self.search_cursor, ch);
        self.search_cursor += ch.len_utf8();
    }

    pub fn search_bar_backspace(&mut self) {
        if self.search_cursor > 0 {
            let prev = self.search_text[..self.search_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.search_text.drain(prev..self.search_cursor);
            self.search_cursor = prev;
        }
    }

    pub fn search_bar_submit(&mut self) {
        let query = self.search_text.trim().to_string();
        if query.is_empty() {
            return;
        }
        // Navigate to Google search
        let url = if query.contains('.') && !query.contains(' ') {
            if query.contains("://") {
                query.clone()
            } else {
                format!("https://{}", query)
            }
        } else {
            format!(
                "https://www.google.com/search?q={}",
                query.replace(' ', "+")
            )
        };
        self.search_text.clear();
        self.search_cursor = 0;
        self.navigate(&url);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE — map WindowId → BrowserState
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    pub static ref BROWSERS: Mutex<BTreeMap<WindowId, BrowserState>> =
        Mutex::new(BTreeMap::new());
}

/// Create a browser state for a new browser window
pub fn create_for_window(wid: WindowId, vivaldi: bool) {
    let state = BrowserState::new(vivaldi);
    BROWSERS.lock().insert(wid, state);
}

/// Destroy browser state when window is closed
pub fn destroy_for_window(wid: WindowId) {
    BROWSERS.lock().remove(&wid);
}

/// Get a reference-safe way to read browser state
pub fn with_browser<F, R>(wid: WindowId, f: F) -> Option<R>
where
    F: FnOnce(&BrowserState) -> R,
{
    let browsers = BROWSERS.lock();
    browsers.get(&wid).map(f)
}

/// Get a reference-safe way to mutate browser state
pub fn with_browser_mut<F, R>(wid: WindowId, f: F) -> Option<R>
where
    F: FnOnce(&mut BrowserState) -> R,
{
    let mut browsers = BROWSERS.lock();
    browsers.get_mut(&wid).map(f)
}

// ═══════════════════════════════════════════════════════════════════════
// CLICK HANDLING
// ═══════════════════════════════════════════════════════════════════════

/// Speed dial site definitions with URLs
pub const SPEED_DIAL_SITES: &[(&str, &str)] = &[
    ("KnoxOS", "knoxos://home"),
    ("GitHub", "https://github.com"),
    ("Vivaldi", "https://vivaldi.com"),
    ("Wikipedia", "https://en.wikipedia.org"),
    ("Reddit", "https://www.reddit.com"),
    ("YouTube", "https://www.youtube.com"),
];

/// Handle a mouse click inside a browser window's content area.
/// `x, y` are absolute screen coordinates.
/// `content` is the window's content rect.
/// Returns true if the click was handled.
pub fn handle_browser_click(
    wid: WindowId,
    x: i32,
    y: i32,
    content_x: i32,
    content_y: i32,
    content_w: u32,
    content_h: u32,
    scroll_y: i32,
) -> bool {
    let mut browsers = BROWSERS.lock();
    let state = match browsers.get_mut(&wid) {
        Some(s) => s,
        None => return false,
    };

    let tab_h: i32 = 28;
    let url_h: i32 = 35;
    let chrome_h = tab_h + url_h;

    // Relative Y within content
    let ry = y - content_y;
    let rx = x - content_x;

    // ─── Tab bar region (0..28) ──────────────────────────────────────
    if ry >= 0 && ry < tab_h {
        // + new tab button (around x=172)
        if (168..=188).contains(&rx) {
            // Open a new tab = navigate to speed dial
            if state.vivaldi {
                state.navigate("vivaldi://newtab");
            } else {
                state.navigate("knoxos://home");
            }
            return true;
        }
        return true; // Consume click on tab bar
    }

    // ─── URL bar region (28..63) ─────────────────────────────────────
    if ry >= tab_h && ry < chrome_h {
        // Navigation buttons (< > O) at x=8..55
        if (0..55).contains(&rx) {
            // Back button area (< at ~8)
            if rx < 20 {
                state.go_back();
                return true;
            }
            // Forward button (> at ~22)
            if rx < 36 {
                state.go_forward();
                return true;
            }
            // Reload button (O at ~40)
            state.reload();
            return true;
        }

        // URL input field (x=60 onwards)
        if rx >= 56 {
            state.focus = BrowserFocus::UrlBar;
            // Place cursor at end or at click position
            // Approximate: each char ~7px wide, offset by 76-content_x
            let text_x = rx - 76;
            if text_x >= 0 {
                let char_idx = (text_x / 7) as usize;
                state.url_cursor = char_idx.min(state.url_text.len());
            } else {
                state.url_cursor = 0;
            }
            return true;
        }
        return true;
    }

    // ─── Page content region (below chrome) ──────────────────────────
    let page_ry = ry - chrome_h + scroll_y;

    // Check if on speed dial page
    if state.page.url == "vivaldi://newtab" || state.page.url == "vivaldi://speeddial" {
        return handle_speed_dial_click(state, rx, page_ry, content_w);
    }

    // Check for clicks on links in loaded page
    if !state.page.lines.is_empty() {
        let line_height: i32 = 20;
        let start_y: i32 = 20; // top margin
        for (i, line) in state.page.lines.iter().enumerate() {
            let ly = start_y + i as i32 * line_height;
            if page_ry >= ly && page_ry < ly + line_height {
                if let Some(ref link_url) = line.link_url {
                    let url = link_url.clone();
                    state.navigate(&url);
                    return true;
                }
                break;
            }
        }
    }

    // Clicking on page content defocuses URL bar
    state.focus = BrowserFocus::Page;
    true
}

/// Handle click on the speed dial page
fn handle_speed_dial_click(
    state: &mut BrowserState,
    rx: i32,
    page_ry: i32,
    content_w: u32,
) -> bool {
    let tile_w: i32 = 140;
    let tile_h: i32 = 100;
    let tile_gap: i32 = 20;
    let grid_cols: i32 = 3;
    let grid_w = grid_cols * (tile_w + tile_gap) - tile_gap;
    let grid_x = (content_w as i32 - grid_w) / 2;
    let grid_y: i32 = 130;

    // Search bar (centered, y=60, height=36)
    let search_w = (content_w.saturating_sub(40)).min(500) as i32;
    let search_x = (content_w as i32 - search_w) / 2;
    if (55..100).contains(&page_ry) && rx >= search_x && rx < search_x + search_w {
        state.focus = BrowserFocus::SearchBar;
        // Place cursor based on click position
        let text_x = rx - search_x - 16;
        if text_x >= 0 {
            let char_idx = (text_x / 7) as usize;
            state.search_cursor = char_idx.min(state.search_text.len());
        } else {
            state.search_cursor = 0;
        }
        return true;
    }

    // Speed dial tiles
    for (i, (_name, url)) in SPEED_DIAL_SITES.iter().enumerate() {
        let col = (i % grid_cols as usize) as i32;
        let row = (i / grid_cols as usize) as i32;
        let tx = grid_x + col * (tile_w + tile_gap);
        let ty = grid_y + row * (tile_h + tile_gap + 20);

        if rx >= tx && rx < tx + tile_w && page_ry >= ty && page_ry < ty + tile_h {
            let target = String::from(*url);
            state.navigate(&target);
            return true;
        }
    }

    // Click on page defocuses
    state.focus = BrowserFocus::Page;
    true
}

// ═══════════════════════════════════════════════════════════════════════
// KEYBOARD HANDLING
// ═══════════════════════════════════════════════════════════════════════

/// Key event types for the browser
#[derive(Debug, Clone)]
pub enum BrowserKey {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Escape,
    Left,
    Right,
    Home,
    End,
    Tab,
    /// Ctrl+L — focus URL bar
    CtrlL,
    /// Ctrl+R — reload
    CtrlR,
    /// Alt+Left — back
    AltLeft,
    /// Alt+Right — forward
    AltRight,
    /// Ctrl+T — new tab (speed dial)
    CtrlT,
    /// Ctrl+D — toggle bookmark for current page
    CtrlD,
}

/// Handle a key event for a browser window.
/// Returns true if the key was consumed.
pub fn handle_browser_key(wid: WindowId, key: BrowserKey) -> bool {
    let mut browsers = BROWSERS.lock();
    let state = match browsers.get_mut(&wid) {
        Some(s) => s,
        None => return false,
    };

    match key {
        BrowserKey::CtrlL => {
            state.focus_url_bar();
            // Select all text for easy replacement
            state.url_cursor = state.url_text.len();
            true
        }
        BrowserKey::CtrlR => {
            state.reload();
            true
        }
        BrowserKey::AltLeft => {
            state.go_back();
            true
        }
        BrowserKey::AltRight => {
            state.go_forward();
            true
        }
        BrowserKey::CtrlT => {
            let url = if state.vivaldi {
                "vivaldi://newtab"
            } else {
                "knoxos://home"
            };
            state.navigate(url);
            true
        }
        BrowserKey::CtrlD => {
            // Toggle bookmark for the current page
            let url = state.page.url.clone();
            let title = state.page.title.clone();
            drop(browsers);
            toggle_bookmark(&title, &url);
            let msg = if is_bookmarked(&url) {
                "Bookmark added"
            } else {
                "Bookmark removed"
            };
            crate::gui::notifications::info("Browser", msg);
            true
        }
        BrowserKey::Escape => {
            if state.focus == BrowserFocus::UrlBar || state.focus == BrowserFocus::SearchBar {
                state.focus = BrowserFocus::Page;
                // Restore URL text to current page URL
                state.url_text = state.page.url.clone();
                state.url_cursor = state.url_text.len();
                true
            } else {
                false
            }
        }
        BrowserKey::Tab => {
            // Tab cycles focus: Page -> UrlBar -> SearchBar -> Page
            match state.focus {
                BrowserFocus::Page => {
                    state.focus = BrowserFocus::UrlBar;
                    state.url_cursor = state.url_text.len();
                }
                BrowserFocus::UrlBar => {
                    if state.page.url.contains("newtab") || state.page.url.contains("speeddial") {
                        state.focus = BrowserFocus::SearchBar;
                    } else {
                        state.focus = BrowserFocus::Page;
                    }
                }
                BrowserFocus::SearchBar => {
                    state.focus = BrowserFocus::Page;
                }
            }
            true
        }
        _ => {
            // Route to focused element
            match state.focus {
                BrowserFocus::UrlBar => handle_url_bar_key(state, key),
                BrowserFocus::SearchBar => handle_search_bar_key(state, key),
                BrowserFocus::Page => handle_page_key(state, key),
            }
        }
    }
}

fn handle_url_bar_key(state: &mut BrowserState, key: BrowserKey) -> bool {
    match key {
        BrowserKey::Char(ch) => {
            state.url_bar_char(ch);
            true
        }
        BrowserKey::Backspace => {
            state.url_bar_backspace();
            true
        }
        BrowserKey::Delete => {
            state.url_bar_delete();
            true
        }
        BrowserKey::Enter => {
            state.url_bar_submit();
            true
        }
        BrowserKey::Left => {
            state.url_bar_left();
            true
        }
        BrowserKey::Right => {
            state.url_bar_right();
            true
        }
        BrowserKey::Home => {
            state.url_bar_home();
            true
        }
        BrowserKey::End => {
            state.url_bar_end();
            true
        }
        _ => false,
    }
}

fn handle_search_bar_key(state: &mut BrowserState, key: BrowserKey) -> bool {
    match key {
        BrowserKey::Char(ch) => {
            state.search_bar_char(ch);
            true
        }
        BrowserKey::Backspace => {
            state.search_bar_backspace();
            true
        }
        BrowserKey::Enter => {
            state.search_bar_submit();
            true
        }
        BrowserKey::Left => {
            if state.search_cursor > 0 {
                state.search_cursor -= 1;
            }
            true
        }
        BrowserKey::Right => {
            if state.search_cursor < state.search_text.len() {
                state.search_cursor += 1;
            }
            true
        }
        _ => false,
    }
}

fn handle_page_key(state: &mut BrowserState, key: BrowserKey) -> bool {
    match key {
        // Typing on the page focuses the URL bar automatically
        BrowserKey::Char(ch) => {
            state.focus_url_bar();
            state.url_text.clear();
            state.url_cursor = 0;
            state.url_bar_char(ch);
            true
        }
        BrowserKey::Backspace => {
            state.go_back();
            true
        }
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BUILT-IN PAGES
// ═══════════════════════════════════════════════════════════════════════

fn make_speed_dial_page() -> LoadedPage {
    LoadedPage {
        url: String::from("vivaldi://newtab"),
        title: String::from("Speed Dial - Vivaldi"),
        lines: Vec::new(), // Speed dial is rendered specially, not as lines
        builtin: true,
        status_code: 0,
        loading: false,
    }
}

fn make_knoxos_home_page() -> LoadedPage {
    let lines = alloc::vec![
        PageLine {
            text: String::from("Welcome to KnoxOS Browser"),
            style: LineStyle::Heading1,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: String::from(
                "KnoxOS is a modern, Linux-compatible operating system built entirely in Rust."
            ),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::from(
                "It features a full desktop environment with window management, a terminal"
            ),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::from("emulator, and a comprehensive GUI toolkit."),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: String::from("Features:"),
            style: LineStyle::Heading2,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} Full desktop environment with taskbar and start menu"),
            style: LineStyle::ListItem,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} Window management with snap, resize, and maximize"),
            style: LineStyle::ListItem,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} Terminal with syntax highlighting and autosuggestion"),
            style: LineStyle::ListItem,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} System notifications and quick settings"),
            style: LineStyle::ListItem,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} File explorer, settings panel, and more"),
            style: LineStyle::ListItem,
            link_url: None
        },
    ];
    LoadedPage {
        url: String::from("knoxos://home"),
        title: String::from("KnoxOS - Home"),
        lines,
        builtin: true,
        status_code: 0,
        loading: false,
    }
}

fn make_settings_page() -> LoadedPage {
    let lines = alloc::vec![
        PageLine {
            text: String::from("Browser Settings"),
            style: LineStyle::Heading1,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: String::from("General"),
            style: LineStyle::Heading2,
            link_url: None
        },
        PageLine {
            text: String::from("  Default search engine: Google"),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::from("  Homepage: vivaldi://newtab"),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: String::from("Privacy & Security"),
            style: LineStyle::Heading2,
            link_url: None
        },
        PageLine {
            text: String::from("  Tracker blocking: Enabled"),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::from("  Ad blocking: Enabled"),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::from("  HTTPS-Only Mode: Enabled"),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: String::from("Appearance"),
            style: LineStyle::Heading2,
            link_url: None
        },
        PageLine {
            text: String::from("  Theme: Dark"),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::from("  Tab bar position: Top"),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::from("  UI zoom: 100%"),
            style: LineStyle::Paragraph,
            link_url: None
        },
    ];
    LoadedPage {
        url: String::from("vivaldi://settings"),
        title: String::from("Settings - Vivaldi"),
        lines,
        builtin: true,
        status_code: 0,
        loading: false,
    }
}

fn make_about_page(vivaldi: bool) -> LoadedPage {
    let lines = if vivaldi {
        alloc::vec![
            PageLine {
                text: String::from("About Vivaldi"),
                style: LineStyle::Heading1,
                link_url: None
            },
            PageLine {
                text: String::new(),
                style: LineStyle::Blank,
                link_url: None
            },
            PageLine {
                text: String::from("Vivaldi 7.1.3570.39 (Official Build)"),
                style: LineStyle::Paragraph,
                link_url: None
            },
            PageLine {
                text: String::from("Revision: a1b2c3d4e5f6"),
                style: LineStyle::Paragraph,
                link_url: None
            },
            PageLine {
                text: format!("OS: KnoxOS 0.6.0 {}", ARCH_NAME),
                style: LineStyle::Paragraph,
                link_url: None
            },
            PageLine {
                text: String::from("Chromium: 132.0.6834.110"),
                style: LineStyle::Paragraph,
                link_url: None
            },
            PageLine {
                text: String::from("JavaScript: V8 13.2.152.16"),
                style: LineStyle::Paragraph,
                link_url: None
            },
            PageLine {
                text: String::new(),
                style: LineStyle::Blank,
                link_url: None
            },
            PageLine {
                text: String::from("Vivaldi is made with love in Oslo, Norway."),
                style: LineStyle::Paragraph,
                link_url: None
            },
            PageLine {
                text: String::from("https://vivaldi.com"),
                style: LineStyle::Link,
                link_url: Some(String::from("https://vivaldi.com"))
            },
        ]
    } else {
        alloc::vec![
            PageLine {
                text: String::from("About KnoxOS Browser"),
                style: LineStyle::Heading1,
                link_url: None
            },
            PageLine {
                text: String::new(),
                style: LineStyle::Blank,
                link_url: None
            },
            PageLine {
                text: String::from("KnoxOS Browser 0.6.0"),
                style: LineStyle::Paragraph,
                link_url: None
            },
            PageLine {
                text: String::from("Built with Rust on bare metal."),
                style: LineStyle::Paragraph,
                link_url: None
            },
        ]
    };
    LoadedPage {
        url: String::from(if vivaldi {
            "vivaldi://about"
        } else {
            "knoxos://about"
        }),
        title: String::from(if vivaldi {
            "About - Vivaldi"
        } else {
            "About - KnoxOS Browser"
        }),
        lines,
        builtin: true,
        status_code: 0,
        loading: false,
    }
}

fn make_history_page(back_stack: &[String]) -> LoadedPage {
    let mut lines = alloc::vec![
        PageLine {
            text: String::from("Browsing History"),
            style: LineStyle::Heading1,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
    ];
    if back_stack.is_empty() {
        lines.push(PageLine {
            text: String::from("No browsing history yet."),
            style: LineStyle::Paragraph,
            link_url: None,
        });
    } else {
        for url in back_stack.iter().rev() {
            lines.push(PageLine {
                text: url.clone(),
                style: LineStyle::Link,
                link_url: Some(url.clone()),
            });
        }
    }
    LoadedPage {
        url: String::from("vivaldi://history"),
        title: String::from("History - Vivaldi"),
        lines,
        builtin: true,
        status_code: 0,
        loading: false,
    }
}

fn make_bookmarks_page() -> LoadedPage {
    let mut lines = Vec::new();
    lines.push(PageLine {
        text: String::from("Bookmarks"),
        style: LineStyle::Heading1,
        link_url: None,
    });
    lines.push(PageLine {
        text: String::new(),
        style: LineStyle::Blank,
        link_url: None,
    });
    lines.push(PageLine {
        text: String::from("Saved Bookmarks"),
        style: LineStyle::Heading2,
        link_url: None,
    });

    // Populate from the dynamic bookmark store
    let bookmarks = BOOKMARKS.lock();
    if bookmarks.is_empty() {
        lines.push(PageLine {
            text: String::from("  No bookmarks saved. Press Ctrl+D on any page to add one."),
            style: LineStyle::Paragraph,
            link_url: None,
        });
    } else {
        for bm in bookmarks.iter() {
            lines.push(PageLine {
                text: format!("  {} — {}", bm.title, bm.url),
                style: LineStyle::Link,
                link_url: Some(bm.url.clone()),
            });
        }
    }
    drop(bookmarks);

    lines.push(PageLine {
        text: String::new(),
        style: LineStyle::Blank,
        link_url: None,
    });
    lines.push(PageLine {
        text: String::from("Tip: Press Ctrl+D to bookmark the current page."),
        style: LineStyle::Paragraph,
        link_url: None,
    });

    LoadedPage {
        url: String::from("vivaldi://bookmarks"),
        title: String::from("Bookmarks - Vivaldi"),
        lines,
        builtin: true,
        status_code: 0,
        loading: false,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HTTP PAGE LOADING
// ═══════════════════════════════════════════════════════════════════════

/// Load a page via HTTP and parse the response into rendered lines
fn load_http_page(url: &str) -> LoadedPage {
    crate::serial_println!("[browser] Loading: {}", url);

    // Parse URL
    let parsed = match parse_browser_url(url) {
        Some(p) => p,
        None => {
            return make_error_page(url, "Invalid URL");
        }
    };

    // Resolve and fetch via kernel HTTP client
    // We reuse the simulated HTTP infrastructure from net.rs
    let result = fetch_page_content(&parsed);

    match result {
        Ok((status, content_type, body)) => {
            let title = extract_title(&body, url);
            let lines = if content_type.contains("text/html") {
                parse_html_to_lines(&body)
            } else if content_type.contains("text/plain") {
                parse_plain_text_to_lines(&body)
            } else {
                alloc::vec![
                    PageLine {
                        text: format!("Content-Type: {}", content_type),
                        style: LineStyle::Paragraph,
                        link_url: None
                    },
                    PageLine {
                        text: format!("Size: {} bytes", body.len()),
                        style: LineStyle::Paragraph,
                        link_url: None
                    },
                    PageLine {
                        text: String::new(),
                        style: LineStyle::Blank,
                        link_url: None
                    },
                    PageLine {
                        text: String::from("(Binary content cannot be displayed)"),
                        style: LineStyle::Paragraph,
                        link_url: None
                    },
                ]
            };

            LoadedPage {
                url: String::from(url),
                title,
                lines,
                builtin: false,
                status_code: status,
                loading: false,
            }
        }
        Err(e) => make_error_page(url, &e),
    }
}

fn make_error_page(url: &str, error: &str) -> LoadedPage {
    let lines = alloc::vec![
        PageLine {
            text: String::from("This site can\u{2019}t be reached"),
            style: LineStyle::Heading1,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: format!("{} is unreachable.", url),
            style: LineStyle::Paragraph,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: format!("Error: {}", error),
            style: LineStyle::Error,
            link_url: None
        },
        PageLine {
            text: String::new(),
            style: LineStyle::Blank,
            link_url: None
        },
        PageLine {
            text: String::from("Try:"),
            style: LineStyle::Heading3,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} Checking the network connection"),
            style: LineStyle::ListItem,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} Checking the proxy and firewall"),
            style: LineStyle::ListItem,
            link_url: None
        },
        PageLine {
            text: String::from("\u{2022} Running Network Diagnostics"),
            style: LineStyle::ListItem,
            link_url: None
        },
    ];
    LoadedPage {
        url: String::from(url),
        title: format!("{} - Error", url),
        lines,
        builtin: false,
        status_code: 0,
        loading: false,
    }
}

/// Simple URL parser for browser navigation
struct BrowserUrl {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn parse_browser_url(url: &str) -> Option<BrowserUrl> {
    let url = url.trim();
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest)
    } else {
        return None;
    };

    let (host_port, path) = match rest.find('/') {
        Some(pos) => (&rest[..pos], &rest[pos..]),
        None => (rest, "/"),
    };

    let (host, port) = match host_port.rfind(':') {
        Some(pos) => {
            let port_str = &host_port[pos + 1..];
            match port_str.parse::<u16>() {
                Ok(p) => (&host_port[..pos], p),
                Err(_) => (host_port, if scheme == "https" { 443 } else { 80 }),
            }
        }
        None => (host_port, if scheme == "https" { 443 } else { 80 }),
    };

    if host.is_empty() {
        return None;
    }

    Some(BrowserUrl {
        scheme: String::from(scheme),
        host: String::from(host),
        port,
        path: String::from(path),
    })
}

/// Fetch page content via the kernel network stack (DNS + TCP socket + HTTP).
/// This goes through the same pipeline as wget/curl shell commands.
/// Returns (status_code, content_type, body_text).
fn fetch_page_content(url: &BrowserUrl) -> Result<(u16, String, String), String> {
    crate::serial_println!(
        "[browser] Fetching via network stack: {}://{}:{}{}",
        url.scheme,
        url.host,
        url.port,
        url.path
    );

    // Step 1: DNS resolution through the kernel DNS subsystem
    let _ip = crate::shell::builtins::net::resolve_host(&url.host)
        .ok_or_else(|| format!("DNS resolution failed for {}", url.host))?;

    crate::serial_println!("[browser] DNS resolved {} -> {}", url.host, _ip);

    // Step 2: Build the full URL and parse it through the net module's URL parser
    let full_url = format!("{}://{}:{}{}", url.scheme, url.host, url.port, url.path);
    let parsed = crate::shell::builtins::net::parse_url(&full_url)
        .map_err(|e| format!("URL parse error: {}", e))?;

    // Step 3: Perform HTTP GET through the full kernel HTTP client
    //   This creates a TCP socket, connects to the resolved IP, sends
    //   an HTTP/1.1 request, and receives the response — all through
    //   the KnoxOS network stack (net::sys_socket, Socket::connect, etc.)
    let result = crate::shell::builtins::net::http_get(
        &parsed,
        &[],  // no extra headers
        true, // follow redirects
        10,   // max 10 redirects
    )
    .map_err(|e| format!("HTTP request failed: {}", e))?;

    crate::serial_println!(
        "[browser] HTTP {} {} - {} bytes, type: {}",
        result.status_code,
        result.status_text,
        result.body.len(),
        result.content_type
    );

    // Step 4: Convert the body bytes to a string for rendering
    let body_text = core::str::from_utf8(&result.body)
        .unwrap_or("<html><body><p>(Binary content cannot be displayed)</p></body></html>");

    Ok((
        result.status_code,
        result.content_type.clone(),
        String::from(body_text),
    ))
}

// ═══════════════════════════════════════════════════════════════════════
// HTML PARSER (simple tag-based)
// ═══════════════════════════════════════════════════════════════════════

/// Extract <title> from HTML
fn extract_title(html: &str, fallback: &str) -> String {
    if let Some(start) = html.find("<title>") {
        let rest = &html[start + 7..];
        if let Some(end) = rest.find("</title>") {
            return String::from(&rest[..end]);
        }
    }
    String::from(fallback)
}

/// Parse HTML into displayable lines with basic tag support
fn parse_html_to_lines(html: &str) -> Vec<PageLine> {
    let mut lines = Vec::new();
    let mut current_text = String::new();
    let mut in_tag = false;
    let mut tag_name = String::new();
    let mut in_body = false;
    let mut in_list = false;
    let mut current_link: Option<String> = None;
    let mut skip_content = false;

    // States for tracking tags
    let mut i = 0;
    let bytes = html.as_bytes();

    while i < bytes.len() {
        let ch = bytes[i] as char;

        if ch == '<' {
            // Start of tag — flush current text
            if in_body && !skip_content {
                let text = current_text.trim().to_string();
                if !text.is_empty() {
                    // Determine style based on context
                    let style = if current_link.is_some() {
                        LineStyle::Link
                    } else if in_list {
                        LineStyle::ListItem
                    } else {
                        LineStyle::Paragraph
                    };
                    lines.push(PageLine {
                        text: if in_list && !text.starts_with('\u{2022}') {
                            format!("\u{2022} {}", text)
                        } else {
                            text
                        },
                        style,
                        link_url: current_link.clone(),
                    });
                }
            }
            current_text.clear();
            in_tag = true;
            tag_name.clear();
            i += 1;
            continue;
        }

        if ch == '>' && in_tag {
            in_tag = false;
            let tag = tag_name.trim().to_lowercase();

            // Process closing tags
            if let Some(close_tag) = tag.strip_prefix('/') {
                match close_tag {
                    "body" => in_body = false,
                    "ul" | "ol" => in_list = false,
                    "a" => current_link = None,
                    "head" | "script" | "style" | "noscript" => skip_content = false,
                    _ => {}
                }
            } else {
                // Process opening tags
                let pure_tag = tag.split_whitespace().next().unwrap_or("");
                match pure_tag {
                    "body" => in_body = true,
                    "head" | "script" | "style" | "noscript" => skip_content = true,
                    "h1" | "h2" | "h3" => {
                        // Heading will be captured as text until closing tag
                    }
                    "ul" | "ol" => in_list = true,
                    "li" => {
                        // List item start
                        current_text.clear();
                    }
                    "br" | "br/"
                        if in_body && !skip_content => {
                            lines.push(PageLine {
                                text: String::new(),
                                style: LineStyle::Blank,
                                link_url: None,
                            });
                        }
                    "p"
                        // Paragraph — add blank line before if we have content
                        if !lines.is_empty() => {
                            if let Some(last) = lines.last() {
                                if last.style != LineStyle::Blank {
                                    lines.push(PageLine {
                                        text: String::new(),
                                        style: LineStyle::Blank,
                                        link_url: None,
                                    });
                                }
                            }
                        }
                    _ => {}
                }

                // Extract href from <a> tags
                if pure_tag == "a" && tag.contains("href=") {
                    if let Some(href_start) = tag.find("href=\"") {
                        let rest = &tag[href_start + 6..];
                        if let Some(href_end) = rest.find('"') {
                            current_link = Some(String::from(&rest[..href_end]));
                        }
                    }
                }

                // Handle heading closing — look ahead for content
                let heading_style = match pure_tag {
                    "h1" => Some(LineStyle::Heading1),
                    "h2" => Some(LineStyle::Heading2),
                    "h3" => Some(LineStyle::Heading3),
                    _ => None,
                };
                if heading_style.is_some() {
                    // Content will be captured; on </hN>, we'll set the style.
                    // For now, track that we need a heading style
                }
            }

            // Check for heading close to set style on last pushed line
            if let Some(close_tag) = tag.strip_prefix('/') {
                let heading_style = match close_tag {
                    "h1" => Some(LineStyle::Heading1),
                    "h2" => Some(LineStyle::Heading2),
                    "h3" => Some(LineStyle::Heading3),
                    _ => None,
                };
                if let Some(style) = heading_style {
                    // The text was already accumulated; push it as a heading
                    let text = current_text.trim().to_string();
                    if !text.is_empty() {
                        lines.push(PageLine {
                            text,
                            style,
                            link_url: None,
                        });
                        current_text.clear();
                    }
                }
            }

            i += 1;
            continue;
        }

        if in_tag {
            tag_name.push(ch);
        } else if in_body && !skip_content {
            // Decode common HTML entities
            if ch == '&' {
                let rest = &html[i..];
                if rest.starts_with("&amp;") {
                    current_text.push('&');
                    i += 5;
                    continue;
                } else if rest.starts_with("&lt;") {
                    current_text.push('<');
                    i += 4;
                    continue;
                } else if rest.starts_with("&gt;") {
                    current_text.push('>');
                    i += 4;
                    continue;
                } else if rest.starts_with("&nbsp;") {
                    current_text.push(' ');
                    i += 6;
                    continue;
                } else if rest.starts_with("&quot;") {
                    current_text.push('"');
                    i += 6;
                    continue;
                } else if rest.starts_with("&#39;") || rest.starts_with("&apos;") {
                    current_text.push('\'');
                    i += if rest.starts_with("&#39;") { 5 } else { 6 };
                    continue;
                } else if rest.starts_with("&mdash;") || rest.starts_with("&#8212;") {
                    current_text.push('\u{2014}');
                    i += 7;
                    continue;
                } else if rest.starts_with("&ndash;") {
                    current_text.push('\u{2013}');
                    i += 7;
                    continue;
                }
            }
            // Collapse whitespace
            if ch == '\n' || ch == '\r' || ch == '\t' {
                if !current_text.ends_with(' ') {
                    current_text.push(' ');
                }
            } else {
                current_text.push(ch);
            }
        }

        i += 1;
    }

    // Flush remaining text
    if !current_text.trim().is_empty() {
        lines.push(PageLine {
            text: current_text.trim().to_string(),
            style: LineStyle::Paragraph,
            link_url: None,
        });
    }

    if lines.is_empty() {
        lines.push(PageLine {
            text: String::from("(Empty page)"),
            style: LineStyle::Paragraph,
            link_url: None,
        });
    }

    lines
}

/// Parse plain text into lines
fn parse_plain_text_to_lines(text: &str) -> Vec<PageLine> {
    text.lines()
        .map(|l| PageLine {
            text: String::from(l),
            style: LineStyle::Code,
            link_url: None,
        })
        .collect()
}
