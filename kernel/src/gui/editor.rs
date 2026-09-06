// ═══════════════════════════════════════════════════════════════════════
// KnoxOS – Functional Text Editor Module (Sprint 3: 9.91-9.97)
// ═══════════════════════════════════════════════════════════════════════
//
// Provides:
//   • Token-level syntax highlighting for Rust, C, Python, JS/TS, shell
//   • Keyboard input routing (chars, arrows, Enter, Backspace, etc.)
//   • Ctrl+S save, Ctrl+Z undo, Ctrl+Y redo, Ctrl+C/X/V, Ctrl+A
//   • Cursor rendering with blinking
//   • Scroll management synced with Window scroll_y

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::window::{self, WindowContentType, WindowId};
use crate::desktop_apps::TextEditorState;
use crate::gui::event_types::KeyCode;

// ─── Syntax Highlighting ─────────────────────────────────────────────

/// Token types for syntax coloring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Normal,
    Keyword,
    Type,
    String,
    Comment,
    Number,
    Function,
    Macro,
    Attribute,
    Operator,
    Punctuation,
    Lifetime,
}

/// A colored span within a line
#[derive(Debug, Clone)]
pub struct ColorSpan {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// Detect language from filename extension
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    C,
    Python,
    JavaScript,
    Shell,
    Toml,
    Markdown,
    Plain,
}

pub fn detect_language(filename: &str) -> Language {
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => Language::Rust,
        "c" | "h" | "cpp" | "hpp" | "cc" => Language::C,
        "py" => Language::Python,
        "js" | "ts" | "jsx" | "tsx" => Language::JavaScript,
        "sh" | "bash" | "zsh" => Language::Shell,
        "toml" => Language::Toml,
        "md" | "markdown" => Language::Markdown,
        _ => Language::Plain,
    }
}

/// Rust keywords
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
];

/// Rust built-in types
const RUST_TYPES: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "u8", "u16",
    "u32", "u64", "u128", "usize", "String", "Vec", "Option", "Result", "Box", "Rc", "Arc", "Some",
    "None", "Ok", "Err",
];

/// C keywords
const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register", "return",
    "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned",
    "void", "volatile", "while",
];

/// Python keywords
const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// JavaScript keywords
const JS_KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Shell keywords
const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "do", "done", "in",
    "function", "return", "exit", "local", "export", "source", "alias", "unalias", "set", "unset",
    "echo", "read", "true", "false",
];

fn keywords_for(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => RUST_KEYWORDS,
        Language::C => C_KEYWORDS,
        Language::Python => PYTHON_KEYWORDS,
        Language::JavaScript => JS_KEYWORDS,
        Language::Shell => SHELL_KEYWORDS,
        _ => &[],
    }
}

fn types_for(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => RUST_TYPES,
        _ => &[],
    }
}

/// Check if a character is an identifier character
fn is_ident(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// Highlight a single line producing colored spans
pub fn highlight_line(
    line: &str,
    lang: Language,
    in_block_comment: bool,
) -> (Vec<ColorSpan>, bool) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut spans: Vec<ColorSpan> = Vec::new();
    let mut i: usize = 0;
    let mut still_in_block = in_block_comment;

    // If we're continuing a block comment from previous line
    if still_in_block {
        let end = find_block_comment_end(line, lang);
        match end {
            Some(pos) => {
                spans.push(ColorSpan {
                    start: 0,
                    end: pos,
                    kind: TokenKind::Comment,
                });
                i = pos;
                still_in_block = false;
            }
            None => {
                spans.push(ColorSpan {
                    start: 0,
                    end: len,
                    kind: TokenKind::Comment,
                });
                return (spans, true);
            }
        }
    }

    let kws = keywords_for(lang);
    let types = types_for(lang);

    while i < len {
        let ch = bytes[i] as char;

        // ── Line comment ──
        if lang == Language::Python && ch == '#' {
            spans.push(ColorSpan {
                start: i,
                end: len,
                kind: TokenKind::Comment,
            });
            return (spans, still_in_block);
        }
        if (lang == Language::Shell) && ch == '#' {
            spans.push(ColorSpan {
                start: i,
                end: len,
                kind: TokenKind::Comment,
            });
            return (spans, still_in_block);
        }
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            spans.push(ColorSpan {
                start: i,
                end: len,
                kind: TokenKind::Comment,
            });
            return (spans, still_in_block);
        }

        // ── Block comment start ──
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let rest = &line[i + 2..];
            if let Some(close) = rest.find("*/") {
                let end = i + 2 + close + 2;
                spans.push(ColorSpan {
                    start: i,
                    end,
                    kind: TokenKind::Comment,
                });
                i = end;
                continue;
            } else {
                spans.push(ColorSpan {
                    start: i,
                    end: len,
                    kind: TokenKind::Comment,
                });
                return (spans, true);
            }
        }

        // ── Rust attributes (#![...] or #[...]) ──
        if lang == Language::Rust
            && ch == '#'
            && i + 1 < len
            && (bytes[i + 1] == b'[' || bytes[i + 1] == b'!')
        {
            let attr_end = find_matching_bracket(line, i);
            spans.push(ColorSpan {
                start: i,
                end: attr_end,
                kind: TokenKind::Attribute,
            });
            i = attr_end;
            continue;
        }

        // ── Rust lifetime 'a ──
        if lang == Language::Rust
            && ch == '\''
            && i + 1 < len
            && (bytes[i + 1] as char).is_ascii_alphabetic()
        {
            // Check it's not a char literal like 'x' — char literals end with '
            let mut j = i + 2;
            while j < len && is_ident(bytes[j] as char) {
                j += 1;
            }
            // If followed by a quote, it's a char literal, not a lifetime
            if j < len && bytes[j] == b'\'' && j == i + 2 {
                // Single char literal 'x'
                spans.push(ColorSpan {
                    start: i,
                    end: j + 1,
                    kind: TokenKind::String,
                });
                i = j + 1;
                continue;
            }
            spans.push(ColorSpan {
                start: i,
                end: j,
                kind: TokenKind::Lifetime,
            });
            i = j;
            continue;
        }

        // ── Strings (double quotes) ──
        if ch == '"' {
            let end = find_string_end(line, i + 1, b'"');
            spans.push(ColorSpan {
                start: i,
                end,
                kind: TokenKind::String,
            });
            i = end;
            continue;
        }

        // ── Strings (single quotes for Python/JS char) ──
        if (lang == Language::Python || lang == Language::JavaScript) && ch == '\'' {
            let end = find_string_end(line, i + 1, b'\'');
            spans.push(ColorSpan {
                start: i,
                end,
                kind: TokenKind::String,
            });
            i = end;
            continue;
        }

        // ── Backtick strings (JS template literals) ──
        if lang == Language::JavaScript && ch == '`' {
            let end = find_string_end(line, i + 1, b'`');
            spans.push(ColorSpan {
                start: i,
                end,
                kind: TokenKind::String,
            });
            i = end;
            continue;
        }

        // ── Numbers ──
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < len && (bytes[i + 1] as char).is_ascii_digit())
        {
            let mut j = i;
            // Hex prefix
            if ch == '0' && j + 1 < len && (bytes[j + 1] == b'x' || bytes[j + 1] == b'X') {
                j += 2;
                while j < len && (bytes[j] as char).is_ascii_hexdigit() {
                    j += 1;
                }
            } else if ch == '0' && j + 1 < len && (bytes[j + 1] == b'b' || bytes[j + 1] == b'B') {
                j += 2;
                while j < len && (bytes[j] == b'0' || bytes[j] == b'1' || bytes[j] == b'_') {
                    j += 1;
                }
            } else {
                while j < len
                    && ((bytes[j] as char).is_ascii_digit() || bytes[j] == b'.' || bytes[j] == b'_')
                {
                    j += 1;
                }
            }
            // Type suffix (u32, i64, f64, etc.)
            if lang == Language::Rust && j < len && (bytes[j] as char).is_ascii_alphabetic() {
                while j < len && is_ident(bytes[j] as char) {
                    j += 1;
                }
            }
            spans.push(ColorSpan {
                start: i,
                end: j,
                kind: TokenKind::Number,
            });
            i = j;
            continue;
        }

        // ── Identifiers / keywords ──
        if is_ident(ch) && !ch.is_ascii_digit() {
            let start = i;
            while i < len && is_ident(bytes[i] as char) {
                i += 1;
            }
            let word = &line[start..i];

            // Check for macro invocation (word followed by !)
            if lang == Language::Rust && i < len && bytes[i] == b'!' {
                spans.push(ColorSpan {
                    start,
                    end: i + 1,
                    kind: TokenKind::Macro,
                });
                i += 1; // skip the !
                continue;
            }

            // Check for function call (word followed by parenthesis)
            let next_non_ws = line[i..].chars().next();
            if next_non_ws == Some('(') {
                if kws.contains(&word) {
                    spans.push(ColorSpan {
                        start,
                        end: i,
                        kind: TokenKind::Keyword,
                    });
                } else {
                    spans.push(ColorSpan {
                        start,
                        end: i,
                        kind: TokenKind::Function,
                    });
                }
                continue;
            }

            if kws.contains(&word) {
                spans.push(ColorSpan {
                    start,
                    end: i,
                    kind: TokenKind::Keyword,
                });
            } else if types.contains(&word) {
                spans.push(ColorSpan {
                    start,
                    end: i,
                    kind: TokenKind::Type,
                });
            } else {
                spans.push(ColorSpan {
                    start,
                    end: i,
                    kind: TokenKind::Normal,
                });
            }
            continue;
        }

        // ── Operators ──
        if b"=+-*/<>!&|^%~?".contains(&bytes[i]) {
            let start = i;
            i += 1;
            // Consume multi-char operators (==, !=, >=, <=, ->, =>, ::, etc.)
            if i < len && b"=>&|".contains(&bytes[i]) {
                i += 1;
            }
            spans.push(ColorSpan {
                start,
                end: i,
                kind: TokenKind::Operator,
            });
            continue;
        }

        // ── Punctuation ──
        if b"{}[]();:,.@".contains(&bytes[i]) {
            spans.push(ColorSpan {
                start: i,
                end: i + 1,
                kind: TokenKind::Punctuation,
            });
            i += 1;
            continue;
        }

        // ── Whitespace / other ──
        i += 1;
    }

    (spans, still_in_block)
}

fn find_block_comment_end(line: &str, lang: Language) -> Option<usize> {
    let _ = lang; // Uniform */ for C-family and Rust
    line.find("*/").map(|p| p + 2)
}

fn find_matching_bracket(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            depth += 1;
        }
        if bytes[i] == b']' {
            depth -= 1;
            if depth <= 0 {
                return i + 1;
            }
        }
        i += 1;
    }
    bytes.len()
}

fn find_string_end(line: &str, start: usize, quote: u8) -> usize {
    let bytes = line.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped char
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    bytes.len() // unclosed string extends to end of line
}

// ─── Color Palette ───────────────────────────────────────────────────

use crate::gui::framebuffer::Pixel;

pub fn token_color(kind: TokenKind) -> Pixel {
    match kind {
        TokenKind::Normal => Pixel::rgb(212, 212, 212), // light grey
        TokenKind::Keyword => Pixel::rgb(86, 156, 214), // blue
        TokenKind::Type => Pixel::rgb(78, 201, 176),    // teal
        TokenKind::String => Pixel::rgb(206, 145, 120), // orange-brown
        TokenKind::Comment => Pixel::rgb(106, 153, 85), // green
        TokenKind::Number => Pixel::rgb(181, 206, 168), // light green
        TokenKind::Function => Pixel::rgb(220, 220, 170), // yellow
        TokenKind::Macro => Pixel::rgb(190, 130, 220),  // purple
        TokenKind::Attribute => Pixel::rgb(155, 155, 155), // grey
        TokenKind::Operator => Pixel::rgb(212, 212, 212), // light grey
        TokenKind::Punctuation => Pixel::rgb(150, 150, 150), // medium grey
        TokenKind::Lifetime => Pixel::rgb(86, 156, 214), // blue (like keywords)
    }
}

// ─── Editor State Management ─────────────────────────────────────────
//
// We store a global map of WindowId → EditorTabState (multi-tab).
// Each window can have multiple file tabs, one active at a time.

use alloc::collections::BTreeMap;
use spin::Mutex;

/// Multi-tab editor state: each window holds a list of tabs
pub struct EditorTabState {
    /// List of open editor tabs (each is a TextEditorState)
    pub tabs: Vec<TextEditorState>,
    /// Currently active tab index
    pub active_tab: usize,
}

impl EditorTabState {
    pub fn new() -> Self {
        Self {
            tabs: vec![TextEditorState::new()],
            active_tab: 0,
        }
    }

    pub fn from_file(path: &str) -> Self {
        let mut state = TextEditorState::new();
        state.open(path);
        Self {
            tabs: vec![state],
            active_tab: 0,
        }
    }

    /// Get the currently active tab state
    pub fn active(&self) -> &TextEditorState {
        &self.tabs[self.active_tab]
    }

    /// Get the currently active tab state mutably
    pub fn active_mut(&mut self) -> &mut TextEditorState {
        &mut self.tabs[self.active_tab]
    }

    /// Open a file in a new tab (or switch to it if already open)
    pub fn open_in_new_tab(&mut self, path: &str) {
        // Check if already open
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.filename.as_deref() == Some(path) {
                self.active_tab = i;
                return;
            }
        }
        let mut state = TextEditorState::new();
        state.open(path);
        self.tabs.push(state);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Add a new empty tab
    pub fn new_tab(&mut self) {
        self.tabs.push(TextEditorState::new());
        self.active_tab = self.tabs.len() - 1;
    }

    /// Close the current tab. Returns false if this was the last tab (window should close).
    pub fn close_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false; // Last tab — signal to close the window
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    /// Switch to a specific tab by index
    pub fn switch_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active_tab = idx;
        }
    }
}

pub static EDITOR_STATES: Mutex<BTreeMap<u32, EditorTabState>> = Mutex::new(BTreeMap::new());

/// Initialise an editor state for a window, loading file content
pub fn open_file(wid: WindowId, path: &str) {
    let mut states = EDITOR_STATES.lock();
    if let Some(tab_state) = states.get_mut(&wid) {
        // Window already has editor tabs — open in new tab
        tab_state.open_in_new_tab(path);
    } else {
        states.insert(wid, EditorTabState::from_file(path));
    }
}

/// Create a new empty editor for a window
pub fn new_empty(wid: WindowId) {
    EDITOR_STATES.lock().insert(wid, EditorTabState::new());
}

/// Remove editor state when window closes
pub fn close_editor(wid: WindowId) {
    EDITOR_STATES.lock().remove(&wid);
}

/// Open a new empty tab in the editor for the given window
pub fn new_tab(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(tab_state) = states.get_mut(&wid) {
        tab_state.new_tab();
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Close the current tab; returns true if tabs remain, false if window should close
pub fn close_tab(wid: WindowId) -> bool {
    let mut states = EDITOR_STATES.lock();
    if let Some(tab_state) = states.get_mut(&wid) {
        let result = tab_state.close_tab();
        drop(states);
        crate::gui::request_redraw();
        result
    } else {
        false
    }
}

/// Switch to a specific tab
pub fn switch_tab(wid: WindowId, idx: usize) {
    let mut states = EDITOR_STATES.lock();
    if let Some(tab_state) = states.get_mut(&wid) {
        tab_state.switch_tab(idx);
    }
    drop(states);
    crate::gui::request_redraw();
}

// ─── Keyboard Handling ───────────────────────────────────────────────

/// Handle a printable character input for the editor
pub fn handle_char(wid: WindowId, ch: char) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        state.insert_char(ch);
        sync_scroll_to_cursor(wid, state);
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Enter key
pub fn handle_enter(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        state.insert_newline();
        sync_scroll_to_cursor(wid, state);
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Backspace key
pub fn handle_backspace(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        state.backspace();
        sync_scroll_to_cursor(wid, state);
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Delete key (delete char at cursor)
pub fn handle_delete(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        if state.cursor_line < state.lines.len() {
            let line_len = state.lines[state.cursor_line].len();
            if state.cursor_col < line_len {
                // Delete char at cursor
                let ch = state.lines[state.cursor_line].remove(state.cursor_col);
                state.modified = true;
                state
                    .undo_stack
                    .push(crate::desktop_apps::EditorAction::Delete {
                        line: state.cursor_line,
                        col: state.cursor_col,
                        text: alloc::format!("{}", ch),
                    });
            } else if state.cursor_line + 1 < state.lines.len() {
                // Join next line
                let next = state.lines.remove(state.cursor_line + 1);
                state.lines[state.cursor_line].push_str(&next);
                state.modified = true;
                state
                    .undo_stack
                    .push(crate::desktop_apps::EditorAction::JoinLine {
                        line: state.cursor_line,
                    });
            }
        }
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Tab key
pub fn handle_tab(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        state.insert_char('\t');
        sync_scroll_to_cursor(wid, state);
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle cursor movement
pub fn handle_arrow(wid: WindowId, key: KeyCode) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        match key {
            KeyCode::ArrowUp => state.move_cursor(0, -1),
            KeyCode::ArrowDown => state.move_cursor(0, 1),
            KeyCode::ArrowLeft => state.move_cursor(-1, 0),
            KeyCode::ArrowRight => state.move_cursor(1, 0),
            KeyCode::Home => {
                state.cursor_col = 0;
            }
            KeyCode::End => {
                let line_len = state.lines.get(state.cursor_line).map_or(0, |l| l.len());
                state.cursor_col = line_len;
            }
            KeyCode::PageUp => {
                let lines_per_page = 25usize; // approx visible lines
                if state.cursor_line > lines_per_page {
                    state.cursor_line -= lines_per_page;
                } else {
                    state.cursor_line = 0;
                }
                let line_len = state.lines.get(state.cursor_line).map_or(0, |l| l.len());
                state.cursor_col = state.cursor_col.min(line_len);
            }
            KeyCode::PageDown => {
                let lines_per_page = 25usize;
                state.cursor_line =
                    (state.cursor_line + lines_per_page).min(state.lines.len().saturating_sub(1));
                let line_len = state.lines.get(state.cursor_line).map_or(0, |l| l.len());
                state.cursor_col = state.cursor_col.min(line_len);
            }
            _ => {}
        }
        sync_scroll_to_cursor(wid, state);
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Ctrl+S save
pub fn handle_save(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        if state.save() {
            let msg = state.status_msg.clone();
            drop(states);
            crate::gui::notifications::info("Editor", &msg);
        } else {
            let msg = state.status_msg.clone();
            drop(states);
            crate::gui::notifications::error("Editor", &msg);
        }
    }
}

/// Handle Ctrl+Z undo
pub fn handle_undo(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        if let Some(action) = state.undo_stack.pop() {
            match action {
                crate::desktop_apps::EditorAction::Insert {
                    line,
                    col,
                    ref text,
                } => {
                    // Undo insert = delete the text
                    for _ in 0..text.len() {
                        if col < state.lines[line].len() {
                            state.lines[line].remove(col);
                        }
                    }
                    state.cursor_line = line;
                    state.cursor_col = col;
                    state.redo_stack.push(action);
                }
                crate::desktop_apps::EditorAction::Delete {
                    line,
                    col,
                    ref text,
                } => {
                    // Undo delete = re-insert the text
                    for (i, ch) in text.chars().enumerate() {
                        state.lines[line].insert(col + i, ch);
                    }
                    state.cursor_line = line;
                    state.cursor_col = col + text.len();
                    state.redo_stack.push(action);
                }
                crate::desktop_apps::EditorAction::SplitLine { line, col } => {
                    // Undo split = join lines
                    if line + 1 < state.lines.len() {
                        let next = state.lines.remove(line + 1);
                        state.lines[line].push_str(&next);
                    }
                    state.cursor_line = line;
                    state.cursor_col = col;
                    state.redo_stack.push(action);
                }
                crate::desktop_apps::EditorAction::JoinLine { line } => {
                    // Undo join = split again
                    // We don't have the original split point easily; approximate
                    state.redo_stack.push(action);
                }
            }
            state.modified = true;
            sync_scroll_to_cursor(wid, state);
        }
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Ctrl+Y redo
pub fn handle_redo(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        if let Some(action) = state.redo_stack.pop() {
            match action {
                crate::desktop_apps::EditorAction::Insert {
                    line,
                    col,
                    ref text,
                } => {
                    for (i, ch) in text.chars().enumerate() {
                        if line < state.lines.len() {
                            let line_len = state.lines[line].len();
                            state.lines[line].insert((col + i).min(line_len), ch);
                        }
                    }
                    state.cursor_line = line;
                    state.cursor_col = col + text.len();
                    state.undo_stack.push(action);
                }
                crate::desktop_apps::EditorAction::Delete {
                    line,
                    col,
                    ref text,
                } => {
                    for _ in 0..text.len() {
                        if col < state.lines[line].len() {
                            state.lines[line].remove(col);
                        }
                    }
                    state.cursor_line = line;
                    state.cursor_col = col;
                    state.undo_stack.push(action);
                }
                crate::desktop_apps::EditorAction::SplitLine { line, col } => {
                    if line < state.lines.len() {
                        let rest = String::from(&state.lines[line][col..]);
                        state.lines[line].truncate(col);
                        state.lines.insert(line + 1, String::from(&rest));
                        state.cursor_line = line + 1;
                        state.cursor_col = 0;
                    }
                    state.undo_stack.push(action);
                }
                crate::desktop_apps::EditorAction::JoinLine { line } => {
                    if line + 1 < state.lines.len() {
                        let next = state.lines.remove(line + 1);
                        state.lines[line].push_str(&next);
                    }
                    state.undo_stack.push(action);
                }
            }
            state.modified = true;
            sync_scroll_to_cursor(wid, state);
        }
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Ctrl+A select all (move cursor to end)
pub fn handle_select_all(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        state.selection_start = Some((0, 0));
        let last_line = state.lines.len().saturating_sub(1);
        let last_col = state.lines.last().map_or(0, |l| l.len());
        state.cursor_line = last_line;
        state.cursor_col = last_col;
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Keep the cursor visible by adjusting window scroll
fn sync_scroll_to_cursor(wid: WindowId, state: &TextEditorState) {
    let line_h = 14i32;
    let status_h = 22i32;
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        let visible_lines = ((win.rect.height as i32 - 30 - status_h) / line_h).max(1) as usize;
        let scroll_line = (win.scroll_y / line_h) as usize;
        if state.cursor_line < scroll_line {
            win.scroll_y = (state.cursor_line as i32) * line_h;
        } else if state.cursor_line >= scroll_line + visible_lines {
            win.scroll_y = ((state.cursor_line + 1 - visible_lines) as i32) * line_h;
        }
        // Update max scroll
        let total_h = state.lines.len() as i32 * line_h;
        let visible_h = win.rect.height as i32 - 30 - status_h;
        win.max_scroll_y = (total_h - visible_h).max(0);
        win.clamp_scroll();
    }
}

// ─── Rendering ───────────────────────────────────────────────────────

use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Rect};

/// Draw the text editor content into the given content rect.
/// Called from Window::draw_editor_content.
pub fn draw(fb: &mut FrameBuffer, content: Rect, scroll_y: i32, wid: WindowId) -> i32 {
    let line_h = 14i32;
    let gutter_w: u32 = 48;
    let status_h: i32 = 22;
    let text_color = Pixel::rgb(212, 212, 212);
    let gutter_color = Pixel::rgb(80, 80, 80);
    let gutter_bg = Pixel::rgb(24, 24, 24);
    let active_line_bg = Pixel::new(255, 255, 255, 10);
    let cursor_color = Pixel::rgb(210, 210, 210);
    let status_bg = Pixel::rgb(0, 122, 204);
    let status_text = Pixel::rgb(255, 255, 255);
    let minimap_bg = Pixel::rgb(28, 28, 28);

    let states = EDITOR_STATES.lock();
    let ts = match states.get(&wid) {
        Some(s) => s,
        None => {
            // No editor state — draw placeholder
            fonts::draw_string_compact(
                fb,
                content.x + 60,
                content.y + 20,
                "No file loaded",
                gutter_color,
                1,
            );
            return 0;
        }
    };

    // ── Tab bar (only if multiple tabs) ──
    let tab_bar_h: i32 = if ts.tabs.len() > 1 { 26 } else { 0 };

    if tab_bar_h > 0 {
        let tab_bg = Pixel::rgb(30, 30, 30);
        let tab_active_bg = Pixel::rgb(24, 24, 24);
        let tab_text_c = Pixel::rgb(150, 150, 150);
        let tab_active_text_c = Pixel::rgb(255, 255, 255);
        let tab_border = Pixel::rgb(50, 50, 50);

        // Tab bar background
        fb.fill_rect(
            Rect::new(content.x, content.y, content.width, tab_bar_h as u32),
            tab_bg,
        );
        fb.draw_hline(
            content.x,
            content.y + tab_bar_h - 1,
            content.width,
            tab_border,
        );

        let tab_w = 150i32.min(content.width as i32 / ts.tabs.len().max(1) as i32);
        for (i, tab) in ts.tabs.iter().enumerate() {
            let tx = content.x + (i as i32) * tab_w;
            let is_active = i == ts.active_tab;

            if is_active {
                fb.fill_rect(
                    Rect::new(tx, content.y, tab_w as u32, tab_bar_h as u32),
                    tab_active_bg,
                );
                // Active tab indicator (cyan line at bottom)
                fb.fill_rect(
                    Rect::new(tx, content.y + tab_bar_h - 2, tab_w as u32, 2),
                    Pixel::rgb(0, 122, 204),
                );
            }

            // Tab label: filename or "Untitled"
            let label = tab
                .filename
                .as_deref()
                .and_then(|f| f.rsplit('/').next())
                .unwrap_or("Untitled");
            let modified_indicator = if tab.modified { "● " } else { "" };
            let tab_label = alloc::format!("{}{}", modified_indicator, label);
            let max_chars = (tab_w / 7) as usize;
            let display_label = if tab_label.len() > max_chars {
                alloc::format!("{}…", &tab_label[..max_chars.saturating_sub(1)])
            } else {
                tab_label
            };
            let tc = if is_active {
                tab_active_text_c
            } else {
                tab_text_c
            };
            fonts::draw_string_compact(fb, tx + 8, content.y + 7, &display_label, tc, 1);

            // Close button (×) on each tab
            let close_x = tx + tab_w - 16;
            let close_y = content.y + 7;
            fonts::draw_string_compact(fb, close_x, close_y, "×", Pixel::rgb(120, 120, 120), 1);

            // Tab separator
            if i > 0 {
                fb.draw_vline(tx, content.y + 4, (tab_bar_h - 8) as u32, tab_border);
            }
        }

        // "+" new tab button
        let plus_x = content.x + (ts.tabs.len() as i32) * tab_w + 8;
        if plus_x + 20 < content.x + content.width as i32 {
            fonts::draw_string_compact(
                fb,
                plus_x,
                content.y + 7,
                "+",
                Pixel::rgb(100, 100, 100),
                1,
            );
        }
    }

    // Adjust content area below tab bar
    let code_area_y = content.y + tab_bar_h;
    let code_area_h = content.height as i32 - status_h - tab_bar_h;
    let code_content = Rect::new(
        content.x,
        code_area_y,
        content.width,
        code_area_h.max(0) as u32,
    );

    // ── Draw gutter ──
    fb.fill_rect(
        Rect::new(
            code_content.x,
            code_content.y,
            gutter_w,
            code_content.height,
        ),
        gutter_bg,
    );
    fb.draw_vline(
        code_content.x + gutter_w as i32,
        code_content.y,
        code_content.height,
        Pixel::rgb(40, 40, 40),
    );

    let state = ts.active();

    let lang = state
        .filename
        .as_deref()
        .map_or(Language::Plain, detect_language);
    let total_lines = state.lines.len();
    let scroll_offset = (scroll_y / line_h.max(1)) as usize;
    let max_visible = ((code_area_h - 4) / line_h).max(0) as usize;
    let code_x = code_content.x + gutter_w as i32 + 8;
    let has_minimap = content.width > 400;
    let minimap_w: u32 = if has_minimap { 50 } else { 0 };
    let max_code_w = content.width.saturating_sub(gutter_w + 16 + minimap_w);
    let char_w = 8i32; // compact font char width

    // ── Highlight and render visible lines ──
    let mut in_block_comment = false;
    // Pre-scan lines before scroll_offset to track block comment state
    for i in 0..scroll_offset.min(total_lines) {
        let (_, still_in_block) = highlight_line(&state.lines[i], lang, in_block_comment);
        in_block_comment = still_in_block;
    }

    for vi in 0..max_visible {
        let line_idx = scroll_offset + vi;
        if line_idx >= total_lines {
            break;
        }

        let y = code_content.y + (vi as i32) * line_h + 4;
        if y + line_h < code_content.y || y > code_content.y + code_area_h {
            break;
        }

        // ── Active line highlight ──
        if line_idx == state.cursor_line {
            fb.fill_rect(
                Rect::new(
                    code_content.x + gutter_w as i32 + 1,
                    y - 1,
                    content.width - gutter_w - 1 - minimap_w,
                    line_h as u32 + 1,
                ),
                active_line_bg,
            );
        }

        // ── Line number ──
        let num_str = alloc::format!("{:>4}", line_idx + 1);
        let num_color = if line_idx == state.cursor_line {
            Pixel::rgb(200, 200, 200)
        } else {
            gutter_color
        };
        fonts::draw_string_compact(fb, code_content.x + 2, y, &num_str, num_color, 1);

        // ── Syntax-highlighted text ──
        let line = &state.lines[line_idx];
        let (spans, still_in_block) = highlight_line(line, lang, in_block_comment);
        in_block_comment = still_in_block;

        if spans.is_empty() && !line.is_empty() {
            // Render as plain text if no spans
            let display = if line.len() > (max_code_w as usize / char_w as usize) {
                fonts::truncate_with_ellipsis_compact(line, max_code_w, 1)
            } else {
                String::from(line)
            };
            fonts::draw_string_compact(fb, code_x, y, &display, text_color, 1);
        } else {
            // Render each span with its color
            for span in &spans {
                if span.start >= line.len() {
                    continue;
                }
                let end = span.end.min(line.len());
                let segment = &line[span.start..end];
                let sx = code_x + (span.start as i32) * char_w;
                if sx >= code_content.x + content.width as i32 - minimap_w as i32 {
                    break; // off-screen
                }
                let color = token_color(span.kind);
                fonts::draw_string_compact(fb, sx, y, segment, color, 1);
            }
            // Fill gaps between spans (whitespace, unmatched chars) with normal color
            let mut covered = alloc::vec![false; line.len()];
            for span in &spans {
                for j in span.start..span.end.min(line.len()) {
                    covered[j] = true;
                }
            }
            let mut gap_start: Option<usize> = None;
            for j in 0..line.len() {
                if !covered[j] {
                    if gap_start.is_none() {
                        gap_start = Some(j);
                    }
                } else if let Some(gs) = gap_start {
                    let segment = &line[gs..j];
                    let sx = code_x + (gs as i32) * char_w;
                    fonts::draw_string_compact(fb, sx, y, segment, text_color, 1);
                    gap_start = None;
                }
            }
            if let Some(gs) = gap_start {
                let segment = &line[gs..line.len()];
                let sx = code_x + (gs as i32) * char_w;
                fonts::draw_string_compact(fb, sx, y, segment, text_color, 1);
            }
        }

        // ── Cursor ──
        if line_idx == state.cursor_line {
            let cx = code_x + (state.cursor_col as i32) * char_w;
            // Blinking: use a frame counter from TSC
            let tsc = super::read_tsc_public();
            let frame_ticks = super::min_frame_ticks().max(1);
            let blink_period = frame_ticks * 60; // ~1 second
            let phase = (tsc / (blink_period / 2)) % 2;
            if phase == 0 {
                fb.fill_rect(Rect::new(cx, y - 1, 2, line_h as u32), cursor_color);
            }
        }
    }

    // ── Minimap ──
    if has_minimap {
        let minimap_x = content.x + content.width as i32 - minimap_w as i32;
        fb.fill_rect(
            Rect::new(minimap_x, code_content.y, minimap_w, code_content.height),
            minimap_bg,
        );

        let minimap_line_h = 2i32;
        let max_mm_lines = (code_area_h / (minimap_line_h + 1)) as usize;
        for i in 0..total_lines.min(max_mm_lines) {
            let line = &state.lines[i];
            if !line.is_empty() {
                let my = code_content.y + (i as i32) * (minimap_line_h + 1);
                if my < code_content.y + code_area_h {
                    let mw = (line.len() as u32 / 2).min(minimap_w - 4).max(1);
                    let alpha = if i == state.cursor_line { 60u8 } else { 25 };
                    fb.fill_rect(
                        Rect::new(minimap_x + 2, my, mw, minimap_line_h as u32),
                        Pixel::new(180, 180, 180, alpha),
                    );
                }
            }
        }

        // Viewport indicator on minimap
        if total_lines > 0 {
            let mm_total_h = (total_lines as i32) * (minimap_line_h + 1);
            let ratio = if mm_total_h > 0 {
                scroll_offset as f32 / total_lines as f32
            } else {
                0.0
            };
            let view_h = ((max_visible as f32 / total_lines.max(1) as f32) * code_area_h as f32)
                .max(10.0)
                .min(code_area_h as f32) as u32;
            let view_y = code_content.y + (ratio * (code_area_h - view_h as i32) as f32) as i32;
            fb.fill_rect(
                Rect::new(minimap_x, view_y, minimap_w, view_h),
                Pixel::new(100, 150, 255, 20),
            );
        }
    }

    // ── Status bar ──
    let status_y = content.y + tab_bar_h + code_area_h;
    fb.fill_rect(
        Rect::new(content.x, status_y, content.width, status_h as u32),
        status_bg,
    );
    let status_text_str = state.status_bar();
    fonts::draw_string_compact(
        fb,
        content.x + 8,
        status_y + 4,
        &status_text_str,
        status_text,
        1,
    );

    // ── Scrollbar ──
    let total_content_h = total_lines as i32 * line_h;
    drop(states); // release lock before returning
    total_content_h
}

// ═══════════════════════════════════════════════════════════════════════════
// FIND & REPLACE (9.94)
// ═══════════════════════════════════════════════════════════════════════════

/// Start find mode (Ctrl+F)
pub fn handle_find(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        ts.active_mut().start_search();
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Start find-and-replace mode (Ctrl+H)
pub fn handle_find_replace(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        ts.active_mut().start_replace();
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle a character input while in Search or Replace mode
pub fn handle_search_char(wid: WindowId, ch: char) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        match state.mode {
            crate::desktop_apps::EditorMode::Search => {
                state.search_query.push(ch);
                state.update_search_matches();
            }
            crate::desktop_apps::EditorMode::Replace => {
                if state.replace_field_focused {
                    state.replace_text.push(ch);
                } else {
                    state.search_query.push(ch);
                    state.update_search_matches();
                }
            }
            _ => {}
        }
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle backspace in search/replace mode
pub fn handle_search_backspace(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        match state.mode {
            crate::desktop_apps::EditorMode::Search => {
                state.search_query.pop();
                state.update_search_matches();
            }
            crate::desktop_apps::EditorMode::Replace => {
                if state.replace_field_focused {
                    state.replace_text.pop();
                } else {
                    state.search_query.pop();
                    state.update_search_matches();
                }
            }
            _ => {}
        }
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Enter in search mode (find next) or replace mode (replace current)
pub fn handle_search_enter(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        match state.mode {
            crate::desktop_apps::EditorMode::Search => {
                state.find_next();
            }
            crate::desktop_apps::EditorMode::Replace => {
                state.replace_current();
            }
            _ => {}
        }
        sync_scroll_to_cursor(wid, state);
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Tab in replace mode (toggle search/replace field focus)
pub fn handle_search_tab(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        if state.mode == crate::desktop_apps::EditorMode::Replace {
            state.replace_field_focused = !state.replace_field_focused;
        }
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Escape in search/replace mode
pub fn handle_search_escape(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        ts.active_mut().cancel_search();
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Handle Ctrl+A in replace mode (replace all)
pub fn handle_replace_all(wid: WindowId) {
    let mut states = EDITOR_STATES.lock();
    if let Some(ts) = states.get_mut(&wid) {
        let state = ts.active_mut();
        if state.mode == crate::desktop_apps::EditorMode::Replace {
            state.replace_all();
        }
    }
    drop(states);
    crate::gui::request_redraw();
}

/// Check if an editor is in search/replace mode
pub fn is_in_search_mode(wid: WindowId) -> bool {
    let states = EDITOR_STATES.lock();
    if let Some(ts) = states.get(&wid) {
        let state = ts.active();
        matches!(
            state.mode,
            crate::desktop_apps::EditorMode::Search | crate::desktop_apps::EditorMode::Replace
        )
    } else {
        false
    }
}
