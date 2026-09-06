pub mod app;
pub mod complete;
pub mod editor;
pub mod highlight;
pub mod suggest;
/// Terminal subsystem — split into submodules for maintainability.
///
/// Submodules:
///   theme      — color themes (Tokyo Night, Solarized, Monokai, Dracula)
///   highlight  — fish-style syntax highlighting & ANSI helpers
///   suggest    — history-based autosuggestion
///   complete   — tab completion engine (commands, paths, variables, flags)
///   editor     — interactive line editor (emacs/vi keybinds, kill ring, undo)
///   app        — TerminalApp state machine, key dispatch, VT rendering
pub mod theme;

// Phase 30+ terminal sub-modules (status.md remaining items)
pub mod clickable_urls;
pub mod sixel;
pub mod split_pane;

// Re-export the main public types so that `crate::terminal::Foo` keeps working.
pub use app::TerminalApp;
pub use app::{HighlightKind, OutputLine, ScrollState};
pub use complete::{Completion, CompletionKind};
pub use editor::{CursorStyle, EditingMode, LineEditor, ViMode};
pub use highlight::{HighlightToken, TokenKind};
pub use theme::TerminalTheme;

use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::Mutex;

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════════
// TERMINAL KEY EVENTS
// ═══════════════════════════════════════════════════════════════════════════

/// Terminal key events (abstracted from raw scancodes)
#[derive(Debug, Clone, Copy)]
pub enum TerminalKey {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    CtrlA,
    CtrlC,
    CtrlD,
    CtrlE,
    CtrlK,
    CtrlL,
    CtrlR,
    CtrlU,
    CtrlW,
    CtrlLeft,
    CtrlRight,
    ShiftPgUp,
    ShiftPgDown,
    // Phase 7 additions
    CtrlY,      // Yank from kill ring
    CtrlZ,      // Undo (in emacs mode)
    CtrlShiftZ, // Redo
    AltY,       // Yank-pop (cycle kill ring)
    CtrlShiftC, // Copy selection
    CtrlShiftV, // Paste from clipboard
    F1,         // Help / mode toggle
}

// ═══════════════════════════════════════════════════════════════════════════
// TERMINAL REGISTRY — Maps window IDs to terminal instances
// ═══════════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Registry of terminal instances keyed by window ID
    static ref TERMINALS: Mutex<BTreeMap<u32, TerminalApp>> = Mutex::new(BTreeMap::new());

    /// Fallback terminal for legacy/single-terminal use
    pub static ref TERMINAL: Mutex<TerminalApp> = Mutex::new(TerminalApp::new(80, 25));
}

/// Initialize the terminal subsystem
pub fn init() {
    let mut term = TERMINAL.lock();
    term.show_welcome();
    serial_println!("[KnoxOS] Terminal initialized (Fish-style UX, Alacritty-inspired renderer)");
    serial_println!("[KnoxOS]   Features: syntax highlighting, autosuggestion, tab completion");
    serial_println!("[KnoxOS]   Features: scrollback (10000 lines), reverse search (Ctrl+R)");
}

/// Create a new terminal instance associated with a window ID
pub fn create_for_window(window_id: u32) {
    let mut app = TerminalApp::new(80, 25);
    app.show_welcome();
    TERMINALS.lock().insert(window_id, app);
    serial_println!(
        "[KnoxOS] Terminal instance created for window {}",
        window_id
    );
}

/// Destroy a terminal instance when its window is closed
pub fn destroy_for_window(window_id: u32) {
    TERMINALS.lock().remove(&window_id);
}

/// Feed a key event to the terminal associated with the focused window
pub fn handle_key_for_window(window_id: u32, key: TerminalKey) {
    let mut terms = TERMINALS.lock();
    if let Some(term) = terms.get_mut(&window_id) {
        term.handle_key(key);
    } else {
        drop(terms);
        // Fallback to legacy global terminal
        TERMINAL.lock().handle_key(key);
    }
}

/// Feed a key event to the terminal (legacy single-terminal mode)
pub fn handle_key(key: TerminalKey) {
    // Route to the focused terminal window if one exists
    let wm = crate::gui::window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        let is_term = wm.windows.iter().any(|w| {
            w.id == wid
                && w.content_type == crate::gui::window::WindowContentType::Terminal
                && w.state != crate::gui::window::WindowState::Minimized
        });
        drop(wm);
        if is_term {
            handle_key_for_window(wid, key);
            return;
        }
    } else {
        drop(wm);
    }
    TERMINAL.lock().handle_key(key);
}

/// Render the terminal for a specific window into a framebuffer region
pub fn render_for_window(window_id: u32, fb: &mut FrameBuffer, rect: Rect) {
    let mut terms = TERMINALS.lock();
    if let Some(term) = terms.get_mut(&window_id) {
        term.render(fb, rect);
    } else {
        drop(terms);
        // Fallback: render the global terminal
        TERMINAL.lock().render(fb, rect);
    }
}

/// Render the terminal into a framebuffer region (legacy)
pub fn render(fb: &mut FrameBuffer, rect: Rect) {
    // If there's a focused terminal window, render that one
    let wm = crate::gui::window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        let is_term = wm.windows.iter().any(|w| {
            w.id == wid && w.content_type == crate::gui::window::WindowContentType::Terminal
        });
        drop(wm);
        if is_term {
            render_for_window(wid, fb, rect);
            return;
        }
    } else {
        drop(wm);
    }
    TERMINAL.lock().render(fb, rect);
}

/// Check if any terminal needs redrawing
pub fn is_dirty() -> bool {
    let terms = TERMINALS.lock();
    for term in terms.values() {
        if term.dirty {
            return true;
        }
    }
    // Don't check the global TERMINAL — it is a legacy fallback whose
    // dirty flag is rarely cleared (no window renders it). Checking it
    // would cause constant full redraws even with no terminal window open.
    false
}

/// Tick all terminal cursor blinks (called from timer ~every 500ms)
pub fn tick_all_blinks() {
    let mut terms = TERMINALS.lock();
    for term in terms.values_mut() {
        term.tick_blink();
    }
    // Don't blink the global TERMINAL — it has no visible window and
    // toggling its dirty flag causes unnecessary full desktop redraws.
}

/// Scroll a specific terminal window's scrollback buffer.
/// `up` = true means scroll up (view older history), false = scroll down.
pub fn scroll_window(window_id: u32, up: bool, lines: usize) {
    let mut terms = TERMINALS.lock();
    if let Some(term) = terms.get_mut(&window_id) {
        if up {
            term.scroll_up(lines);
        } else {
            term.scroll_down(lines);
        }
    }
}

/// Get the current prompt for display
pub fn get_prompt() -> String {
    TERMINAL.lock().prompt.clone()
}

/// Create a new terminal tab for a window, returning the new terminal ID
pub fn create_tab(window_id: u32, tab_index: usize) -> u32 {
    let tab_id = window_id * 1000 + tab_index as u32;
    let mut app = TerminalApp::new(80, 25);
    app.show_welcome();
    TERMINALS.lock().insert(tab_id, app);
    serial_println!(
        "[KnoxOS] Terminal tab {} created for window {} (id={})",
        tab_index,
        window_id,
        tab_id
    );
    tab_id
}

/// Destroy a terminal tab
pub fn destroy_tab(tab_id: u32) {
    TERMINALS.lock().remove(&tab_id);
    serial_println!("[KnoxOS] Terminal tab destroyed (id={})", tab_id);
}

/// Handle Ctrl+Click on a terminal window (URL opening)
pub fn handle_ctrl_click(window_id: u32, x: i32, y: i32, rect: crate::gui::framebuffer::Rect) {
    let terms = TERMINALS.lock();
    if let Some(term) = terms.get(&window_id) {
        term.handle_ctrl_click(x, y, rect);
    }
}
