/// Fish-style interactive line editing with vi/emacs keybinds
use alloc::string::String;
use alloc::vec::Vec;

use super::complete::{self, Completion};
use super::highlight::{self, HighlightToken};
use super::suggest;
use crate::shell;

// ═══════════════════════════════════════════════════════════════════════════
// LINE EDITOR — Fish-style interactive line editing with vi/emacs keybinds
// ═══════════════════════════════════════════════════════════════════════════

/// Cursor style for the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Underline,
    Bar,
}

/// Vi editing mode states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViMode {
    /// Normal (command) mode — motions, operators
    Normal,
    /// Insert mode — character insertion
    Insert,
}

/// Editing mode for the line editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditingMode {
    Emacs,
    Vi(ViMode),
}

/// Snapshot of editor state for undo
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    pub buffer: String,
    pub cursor: usize,
}

/// State of the interactive line editor
pub struct LineEditor {
    /// Current input buffer
    pub buffer: String,
    /// Cursor position within buffer (byte offset)
    pub cursor: usize,
    /// Current autosuggestion (ghost text shown after cursor)
    pub suggestion: Option<String>,
    /// Syntax-highlighted tokens for current buffer
    pub tokens: Vec<HighlightToken>,
    /// History position for up/down navigation
    pub history_index: Option<usize>,
    /// Saved buffer when navigating history
    pub saved_buffer: String,
    /// Current tab completions
    pub completions: Vec<Completion>,
    /// Index into completions list (cycling with tab)
    pub completion_index: Option<usize>,
    /// Whether completion menu is showing
    pub showing_completions: bool,
    /// Cursor style
    pub cursor_style: CursorStyle,
    /// Search mode (Ctrl+R reverse search)
    pub search_mode: bool,
    /// Search query
    pub search_query: String,
    /// Search result index
    pub search_result: Option<usize>,
    // ── Phase 7 additions ──
    /// Kill ring — stores killed text for Ctrl+Y yank
    pub kill_ring: Vec<String>,
    /// Current kill ring index for Alt+Y cycling
    pub kill_ring_index: usize,
    /// Whether last action was a yank (for Alt+Y cycling)
    pub last_was_yank: bool,
    /// Length of last yanked text (for Alt+Y replacement)
    pub last_yank_len: usize,
    /// Editing mode (emacs or vi)
    pub editing_mode: EditingMode,
    /// Undo stack — snapshots before each edit
    pub undo_stack: Vec<EditorSnapshot>,
    /// Redo stack — snapshots after undo
    pub redo_stack: Vec<EditorSnapshot>,
    /// Multi-line editing: accumulated lines when `\` continuation is used
    pub continuation_lines: Vec<String>,
    /// Whether we're waiting for continuation input
    pub continuation_pending: bool,
}

impl Default for LineEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            suggestion: None,
            tokens: Vec::new(),
            history_index: None,
            saved_buffer: String::new(),
            completions: Vec::new(),
            completion_index: None,
            showing_completions: false,
            cursor_style: CursorStyle::Block,
            search_mode: false,
            search_query: String::new(),
            search_result: None,
            kill_ring: Vec::new(),
            kill_ring_index: 0,
            last_was_yank: false,
            last_yank_len: 0,
            editing_mode: EditingMode::Emacs,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            continuation_lines: Vec::new(),
            continuation_pending: false,
        }
    }

    /// Reset the editor for a new command
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.suggestion = None;
        self.tokens.clear();
        self.history_index = None;
        self.saved_buffer.clear();
        self.completions.clear();
        self.completion_index = None;
        self.showing_completions = false;
        self.search_mode = false;
        self.search_query.clear();
        self.search_result = None;
        self.last_was_yank = false;
        self.last_yank_len = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.continuation_lines.clear();
        self.continuation_pending = false;
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, ch: char) {
        self.save_undo();
        if self.cursor >= self.buffer.len() {
            self.buffer.push(ch);
        } else {
            self.buffer.insert(self.cursor, ch);
        }
        self.cursor += ch.len_utf8();
        self.last_was_yank = false;
        self.update_highlight();
        self.update_suggestion();
        self.dismiss_completions();
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.save_undo();
            let prev_char_start = self.buffer[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.buffer.remove(prev_char_start);
            self.cursor = prev_char_start;
            self.last_was_yank = false;
            self.update_highlight();
            self.update_suggestion();
            self.dismiss_completions();
        }
    }

    /// Delete character at cursor (delete key)
    pub fn delete(&mut self) {
        if self.cursor < self.buffer.len() {
            self.save_undo();
            self.buffer.remove(self.cursor);
            self.last_was_yank = false;
            self.update_highlight();
            self.update_suggestion();
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            if let Some((_, ch)) = self.buffer[self.cursor..].char_indices().next() {
                self.cursor += ch.len_utf8();
            }
        }
    }

    /// Move cursor to start of line (Home / Ctrl+A)
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of line (End / Ctrl+E)
    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Move cursor to start of previous word (Ctrl+Left / Alt+B)
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let bytes = self.buffer.as_bytes();
        let mut pos = self.cursor - 1;
        // Skip whitespace
        while pos > 0 && bytes[pos] == b' ' {
            pos -= 1;
        }
        // Skip word characters
        while pos > 0 && bytes[pos - 1] != b' ' {
            pos -= 1;
        }
        self.cursor = pos;
    }

    /// Move cursor to end of next word (Ctrl+Right / Alt+F)
    pub fn move_word_right(&mut self) {
        let bytes = self.buffer.as_bytes();
        let len = bytes.len();
        let mut pos = self.cursor;
        // Skip current word
        while pos < len && bytes[pos] != b' ' {
            pos += 1;
        }
        // Skip whitespace
        while pos < len && bytes[pos] == b' ' {
            pos += 1;
        }
        self.cursor = pos;
    }

    /// Delete from cursor to end of line (Ctrl+K)
    pub fn kill_to_end(&mut self) {
        self.save_undo();
        let killed = String::from(&self.buffer[self.cursor..]);
        if !killed.is_empty() {
            self.push_kill_ring(killed);
        }
        self.buffer.truncate(self.cursor);
        self.last_was_yank = false;
        self.update_highlight();
        self.update_suggestion();
    }

    /// Delete from cursor to start of line (Ctrl+U)
    pub fn kill_to_start(&mut self) {
        self.save_undo();
        let killed = String::from(&self.buffer[..self.cursor]);
        if !killed.is_empty() {
            self.push_kill_ring(killed);
        }
        self.buffer = String::from(&self.buffer[self.cursor..]);
        self.cursor = 0;
        self.last_was_yank = false;
        self.update_highlight();
        self.update_suggestion();
    }

    /// Delete previous word (Ctrl+W)
    pub fn kill_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.save_undo();
        let old_cursor = self.cursor;
        self.move_word_left();
        let new_cursor = self.cursor;
        let killed = String::from(&self.buffer[new_cursor..old_cursor]);
        if !killed.is_empty() {
            self.push_kill_ring(killed);
        }
        self.buffer = alloc::format!(
            "{}{}",
            &self.buffer[..new_cursor],
            &self.buffer[old_cursor..]
        );
        self.last_was_yank = false;
        self.update_highlight();
        self.update_suggestion();
    }

    /// Accept the autosuggestion (Right arrow at end of line, or Ctrl+F)
    pub fn accept_suggestion(&mut self) -> bool {
        if let Some(ref suggestion) = self.suggestion.clone() {
            self.buffer = suggestion.clone();
            self.cursor = self.buffer.len();
            self.suggestion = None;
            self.update_highlight();
            true
        } else {
            false
        }
    }

    /// Accept the first word of the suggestion (Alt+Right)
    pub fn accept_suggestion_word(&mut self) -> bool {
        if let Some(ref suggestion) = self.suggestion.clone() {
            let remaining = &suggestion[self.buffer.len()..];
            // Find the end of the next word
            let word_end = remaining
                .find(' ')
                .map(|i| i + 1)
                .unwrap_or(remaining.len());
            let accepted = &remaining[..word_end];
            self.buffer.push_str(accepted);
            self.cursor = self.buffer.len();
            self.update_highlight();
            self.update_suggestion();
            true
        } else {
            false
        }
    }

    /// Navigate history up
    pub fn history_prev(&mut self) {
        let history = shell::get_history();
        if history.is_empty() {
            return;
        }

        let idx = match self.history_index {
            Some(0) => return,
            Some(i) => i - 1,
            None => {
                self.saved_buffer = self.buffer.clone();
                history.len() - 1
            }
        };

        self.history_index = Some(idx);
        self.buffer = history[idx].clone();
        self.cursor = self.buffer.len();
        self.update_highlight();
        self.suggestion = None;
    }

    /// Navigate history down
    pub fn history_down(&mut self) {
        let history = shell::get_history();

        if let Some(i) = self.history_index {
            if i + 1 < history.len() {
                self.history_index = Some(i + 1);
                self.buffer = history[i + 1].clone();
            } else {
                self.history_index = None;
                self.buffer = self.saved_buffer.clone();
            }
            self.cursor = self.buffer.len();
            self.update_highlight();
            self.update_suggestion();
        }
    }

    /// Trigger tab completion
    pub fn tab_complete(&mut self) {
        if self.showing_completions {
            // Cycle through completions
            if let Some(ref mut idx) = self.completion_index {
                *idx = (*idx + 1) % self.completions.len();
            }
        } else {
            // Generate completions
            self.completions = complete::complete(&self.buffer, self.cursor);
            if self.completions.is_empty() {
                return;
            }
            self.completion_index = Some(0);
            self.showing_completions = true;
        }

        // Apply the current completion
        if let Some(idx) = self.completion_index {
            if idx < self.completions.len() {
                let completion = &self.completions[idx];
                // Find the word being completed
                let parts: Vec<&str> = self.buffer[..self.cursor].split_whitespace().collect();
                let prefix = parts.last().copied().unwrap_or("");

                if !prefix.is_empty() {
                    // Replace the last word with the completion
                    let before_word = self.buffer[..self.cursor].rfind(prefix).unwrap_or(0);
                    let after = if self.cursor < self.buffer.len() {
                        String::from(&self.buffer[self.cursor..])
                    } else {
                        String::new()
                    };
                    self.buffer = alloc::format!(
                        "{}{}{}",
                        &self.buffer[..before_word],
                        completion.text,
                        after
                    );
                    self.cursor = before_word + completion.text.len();
                } else {
                    // No prefix, insert the completion
                    let before = String::from(&self.buffer[..self.cursor]);
                    let after = String::from(&self.buffer[self.cursor..]);
                    self.buffer = alloc::format!("{}{}{}", before, completion.text, after);
                    self.cursor += completion.text.len();
                }
                self.update_highlight();
            }
        }
    }

    /// Toggle reverse search mode (Ctrl+R)
    pub fn toggle_search(&mut self) {
        self.search_mode = !self.search_mode;
        if self.search_mode {
            self.search_query.clear();
            self.search_result = None;
        }
    }

    /// Dismiss the completion menu
    pub fn dismiss_completions(&mut self) {
        self.showing_completions = false;
        self.completion_index = None;
        self.completions.clear();
    }

    /// Update syntax highlighting
    pub fn update_highlight(&mut self) {
        self.tokens = highlight::highlight_line(&self.buffer);
    }

    /// Update autosuggestion from history
    pub fn update_suggestion(&mut self) {
        let history = shell::get_history();
        self.suggestion = suggest::find_suggestion(&self.buffer, &history);
    }

    /// Get the submitted command (if Enter was pressed)
    pub fn submit(&mut self) -> String {
        let cmd = self.buffer.clone();
        self.reset();
        cmd
    }

    /// Clear the current input (Ctrl+C)
    pub fn cancel(&mut self) {
        self.reset();
    }

    // ── Kill Ring ──────────────────────────────────────────────

    /// Push text onto the kill ring (max 32 entries)
    fn push_kill_ring(&mut self, text: String) {
        self.kill_ring.push(text);
        if self.kill_ring.len() > 32 {
            self.kill_ring.remove(0);
        }
        self.kill_ring_index = self.kill_ring.len().saturating_sub(1);
    }

    /// Yank (paste) the last killed text at cursor (Ctrl+Y)
    pub fn yank(&mut self) {
        if self.kill_ring.is_empty() {
            return;
        }
        self.save_undo();
        let idx = self.kill_ring_index;
        let text = self.kill_ring[idx].clone();
        let text_len = text.len();
        let before = String::from(&self.buffer[..self.cursor]);
        let after = String::from(&self.buffer[self.cursor..]);
        self.buffer = alloc::format!("{}{}{}", before, text, after);
        self.cursor += text_len;
        self.last_was_yank = true;
        self.last_yank_len = text_len;
        self.update_highlight();
        self.update_suggestion();
    }

    /// Cycle through kill ring entries (Alt+Y) — replaces last yanked text
    pub fn yank_pop(&mut self) {
        if !self.last_was_yank || self.kill_ring.is_empty() {
            return;
        }
        self.save_undo();
        // Remove the previously yanked text
        let yank_start = self.cursor - self.last_yank_len;
        self.buffer = alloc::format!(
            "{}{}",
            &self.buffer[..yank_start],
            &self.buffer[self.cursor..]
        );
        self.cursor = yank_start;
        // Cycle backwards in kill ring
        if self.kill_ring_index == 0 {
            self.kill_ring_index = self.kill_ring.len() - 1;
        } else {
            self.kill_ring_index -= 1;
        }
        // Insert the new kill ring entry
        let text = self.kill_ring[self.kill_ring_index].clone();
        let text_len = text.len();
        let before = String::from(&self.buffer[..self.cursor]);
        let after = String::from(&self.buffer[self.cursor..]);
        self.buffer = alloc::format!("{}{}{}", before, text, after);
        self.cursor += text_len;
        self.last_yank_len = text_len;
        self.update_highlight();
    }

    // ── Undo / Redo ───────────────────────────────────────────

    /// Save current state to undo stack (called before edits)
    pub fn save_undo(&mut self) {
        self.undo_stack.push(EditorSnapshot {
            buffer: self.buffer.clone(),
            cursor: self.cursor,
        });
        // Cap undo stack at 100 entries
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        // Clear redo stack on new edit
        self.redo_stack.clear();
    }

    /// Undo the last edit
    pub fn undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            // Save current state to redo stack
            self.redo_stack.push(EditorSnapshot {
                buffer: self.buffer.clone(),
                cursor: self.cursor,
            });
            self.buffer = snapshot.buffer;
            self.cursor = snapshot.cursor;
            self.update_highlight();
            self.update_suggestion();
        }
    }

    /// Redo the last undone edit
    pub fn redo(&mut self) {
        if let Some(snapshot) = self.redo_stack.pop() {
            self.undo_stack.push(EditorSnapshot {
                buffer: self.buffer.clone(),
                cursor: self.cursor,
            });
            self.buffer = snapshot.buffer;
            self.cursor = snapshot.cursor;
            self.update_highlight();
            self.update_suggestion();
        }
    }

    // ── Vi Mode ───────────────────────────────────────────────

    /// Toggle between emacs and vi editing modes
    pub fn toggle_vi_mode(&mut self) {
        self.editing_mode = match self.editing_mode {
            EditingMode::Emacs => {
                self.cursor_style = CursorStyle::Block;
                EditingMode::Vi(ViMode::Normal)
            }
            EditingMode::Vi(_) => {
                self.cursor_style = CursorStyle::Bar;
                EditingMode::Emacs
            }
        };
    }

    /// Handle a key in vi normal mode. Returns true if the key was consumed.
    pub fn vi_normal_key(&mut self, ch: char) -> bool {
        match ch {
            'i' => {
                self.editing_mode = EditingMode::Vi(ViMode::Insert);
                self.cursor_style = CursorStyle::Bar;
                true
            }
            'a' => {
                self.move_right();
                self.editing_mode = EditingMode::Vi(ViMode::Insert);
                self.cursor_style = CursorStyle::Bar;
                true
            }
            'A' => {
                self.move_end();
                self.editing_mode = EditingMode::Vi(ViMode::Insert);
                self.cursor_style = CursorStyle::Bar;
                true
            }
            'I' => {
                self.move_home();
                self.editing_mode = EditingMode::Vi(ViMode::Insert);
                self.cursor_style = CursorStyle::Bar;
                true
            }
            'h' => {
                self.move_left();
                true
            }
            'l' => {
                self.move_right();
                true
            }
            'w' => {
                self.move_word_right();
                true
            }
            'b' => {
                self.move_word_left();
                true
            }
            '0' => {
                self.move_home();
                true
            }
            '$' => {
                self.move_end();
                true
            }
            'x' => {
                if self.cursor < self.buffer.len() {
                    self.save_undo();
                    let ch = self.buffer.remove(self.cursor);
                    self.push_kill_ring(String::from(ch));
                    if self.cursor > 0 && self.cursor >= self.buffer.len() {
                        self.cursor = self.buffer.len().saturating_sub(1);
                    }
                    self.update_highlight();
                }
                true
            }
            'X' => {
                if self.cursor > 0 {
                    self.backspace();
                }
                true
            }
            'D' => {
                self.kill_to_end();
                true
            }
            'C' => {
                self.kill_to_end();
                self.editing_mode = EditingMode::Vi(ViMode::Insert);
                self.cursor_style = CursorStyle::Bar;
                true
            }
            'S' | 'c' => {
                // cc / S: clear line, enter insert
                self.save_undo();
                let killed = self.buffer.clone();
                if !killed.is_empty() {
                    self.push_kill_ring(killed);
                }
                self.buffer.clear();
                self.cursor = 0;
                self.editing_mode = EditingMode::Vi(ViMode::Insert);
                self.cursor_style = CursorStyle::Bar;
                self.update_highlight();
                true
            }
            'p' => {
                // Paste after cursor
                self.yank();
                true
            }
            'u' => {
                self.undo();
                true
            }
            'k' => {
                self.history_prev();
                true
            }
            'j' => {
                self.history_down();
                true
            }
            _ => false,
        }
    }

    /// Switch vi from insert to normal mode (Escape in vi insert mode)
    pub fn vi_escape_to_normal(&mut self) {
        self.editing_mode = EditingMode::Vi(ViMode::Normal);
        self.cursor_style = CursorStyle::Block;
        // Move cursor back one (vi behavior)
        if self.cursor > 0 {
            self.move_left();
        }
    }

    // ── Multi-line editing ────────────────────────────────────

    /// Check if the current buffer ends with a continuation marker (\)
    pub fn needs_continuation(&self) -> bool {
        let trimmed = self.buffer.trim_end();
        trimmed.ends_with('\\')
    }

    /// Save current line as continuation, reset for next line input
    pub fn start_continuation(&mut self) {
        // Remove trailing backslash
        let mut line = self.buffer.clone();
        if line.trim_end().ends_with('\\') {
            let pos = line.rfind('\\').unwrap();
            line.truncate(pos);
        }
        self.continuation_lines.push(line);
        self.continuation_pending = true;
        self.buffer.clear();
        self.cursor = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.update_highlight();
    }

    /// Get the full multi-line command (joining continuation lines)
    pub fn get_full_command(&mut self) -> String {
        if self.continuation_lines.is_empty() {
            return self.buffer.clone();
        }
        let mut full = String::new();
        for line in &self.continuation_lines {
            full.push_str(line);
        }
        full.push_str(&self.buffer);
        self.continuation_lines.clear();
        self.continuation_pending = false;
        full
    }
}
