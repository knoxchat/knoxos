/// Main terminal application state machine
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::shell;
use crate::vterm::{self, Cell, Color, VtEmulator};

use super::TerminalKey;
use super::clickable_urls;
use super::complete::CompletionKind;
use super::editor::{CursorStyle, EditingMode, LineEditor, ViMode};
use super::highlight::{strip_ansi_visible_len, token_color};
use super::theme::TerminalTheme;

// ═══════════════════════════════════════════════════════════════════════════
// TERMINAL APPLICATION — Main terminal state machine
// ═══════════════════════════════════════════════════════════════════════════

/// Terminal scroll position
#[derive(Debug, Clone, Copy)]
pub struct ScrollState {
    /// Lines scrolled back from bottom (0 = at bottom)
    pub offset: usize,
    /// Total scrollback lines available
    pub total_scrollback: usize,
}

/// Output line in the scrollback buffer
#[derive(Debug, Clone)]
pub struct OutputLine {
    pub cells: Vec<(char, HighlightKind)>,
}

/// How to render a character
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Normal,
    Prompt,
    Command,
    InvalidCommand,
    Argument,
    Flag,
    Path,
    StrLiteral,
    Variable,
    Operator,
    Comment,
    Suggestion,
    Error,
    Info,
    Success,
}

/// The main terminal application state
pub struct TerminalApp {
    /// VT100 emulator for escape sequence processing
    pub vt: VtEmulator,
    /// Interactive line editor with fish-like features
    pub editor: LineEditor,
    /// Terminal theme
    pub theme: TerminalTheme,
    /// Scroll state
    pub scroll: ScrollState,
    /// Scrollback buffer (lines that have scrolled off the top)
    pub scrollback: Vec<Vec<Cell>>,
    /// Maximum scrollback lines
    pub max_scrollback: usize,
    /// Whether we're currently accepting input (vs. showing output)
    pub input_mode: bool,
    /// The prompt string
    pub prompt: String,
    /// Whether the terminal needs redrawing
    pub dirty: bool,
    /// Whether the cursor should blink
    pub cursor_blink: bool,
    /// Cursor blink counter (for animation)
    pub blink_counter: u32,
    /// Terminal session ID for PTY association
    pub pty_id: u32,
    /// Accumulated output lines count
    pub total_lines: usize,
    /// Welcome message displayed
    pub welcome_shown: bool,
    /// Active tmux session ID (None = not in mux mode)
    pub mux_session_id: Option<u32>,
}

impl TerminalApp {
    /// Create a new terminal application
    pub fn new(cols: usize, rows: usize) -> Self {
        let pty_id = crate::tty::alloc_pty();

        Self {
            vt: VtEmulator::new(cols, rows),
            editor: LineEditor::new(),
            theme: TerminalTheme::default(),
            scroll: ScrollState {
                offset: 0,
                total_scrollback: 0,
            },
            scrollback: Vec::new(),
            max_scrollback: 10000,
            input_mode: true,
            prompt: shell::get_prompt(),
            dirty: true,
            cursor_blink: true,
            blink_counter: 0,
            pty_id,
            total_lines: 0,
            welcome_shown: false,
            mux_session_id: None,
        }
    }

    /// Display the welcome message
    pub fn show_welcome(&mut self) {
        if self.welcome_shown {
            return;
        }
        self.welcome_shown = true;

        let welcome = "\
            \x1b[1;34m╔══════════════════════════════════════════════════════╗\x1b[0m\n\
             \x1b[1;34m║\x1b[0m  \x1b[1;36mKnoxOS Terminal\x1b[0m v0.1.0                              \x1b[1;34m║\x1b[0m\n\
             \x1b[1;34m║\x1b[0m                                                      \x1b[1;34m║\x1b[0m\n\
             \x1b[1;34m║\x1b[0m  \x1b[33m•\x1b[0m Smart autosuggestions from history (\x1b[2mRight→\x1b[0m accept) \x1b[1;34m║\x1b[0m\n\
             \x1b[1;34m║\x1b[0m  \x1b[33m•\x1b[0m Syntax highlighting (\x1b[32mvalid\x1b[0m/\x1b[31minvalid\x1b[0m commands)     \x1b[1;34m║\x1b[0m\n\
             \x1b[1;34m║\x1b[0m  \x1b[33m•\x1b[0m Tab completion for commands, files, paths      \x1b[1;34m║\x1b[0m\n\
             \x1b[1;34m║\x1b[0m  \x1b[33m•\x1b[0m History search with \x1b[2mCtrl+R\x1b[0m                     \x1b[1;34m║\x1b[0m\n\
             \x1b[1;34m║\x1b[0m  \x1b[33m•\x1b[0m Scrollback buffer (Shift+PgUp/PgDn)            \x1b[1;34m║\x1b[0m\n\
             \x1b[1;34m╚══════════════════════════════════════════════════════╝\x1b[0m\n\
             \n\
             Type \x1b[1;32mhelp\x1b[0m for available commands.\n\n";

        self.write_output(welcome);
    }

    /// Write output text (with ANSI escape sequences) to the terminal
    pub fn write_output(&mut self, text: &str) {
        self.vt.process_str(text);
        self.dirty = true;
    }

    /// Process raw bytes through the VT emulator
    pub fn write_bytes(&mut self, data: &[u8]) {
        self.vt.process_bytes(data);
        self.dirty = true;
    }

    /// Handle a key press event
    pub fn handle_key(&mut self, key: TerminalKey) {
        self.dirty = true;

        // Scroll to bottom on any input
        self.scroll.offset = 0;

        // Vi normal mode: intercept keys before regular dispatch
        if let EditingMode::Vi(ViMode::Normal) = self.editor.editing_mode {
            match key {
                TerminalKey::Char(ch) if self.editor.vi_normal_key(ch) => {
                    return;
                }
                TerminalKey::Escape => return, // Already in normal mode
                TerminalKey::Enter => {
                    // Submit in normal mode too
                }
                _ => {} // Fall through to regular handling
            }
        }

        match key {
            TerminalKey::Char(ch) => {
                if self.editor.search_mode {
                    self.editor.search_query.push(ch);
                    self.search_history();
                } else {
                    self.editor.insert_char(ch);
                }
            }
            TerminalKey::Enter => {
                if self.editor.search_mode {
                    self.editor.search_mode = false;
                    // Apply search result
                    if let Some(idx) = self.editor.search_result {
                        let history = shell::get_history();
                        if idx < history.len() {
                            self.editor.buffer = history[idx].clone();
                            self.editor.cursor = self.editor.buffer.len();
                        }
                    }
                } else if self.editor.needs_continuation() {
                    // Multi-line: save current line, show PS2 prompt
                    self.editor.start_continuation();
                    let ps2 = shell::ENV_VARS
                        .lock()
                        .get("PS2")
                        .cloned()
                        .unwrap_or_else(|| String::from("> "));
                    self.write_output(&ps2);
                } else {
                    let cmd = if self.editor.continuation_pending {
                        self.editor.get_full_command()
                    } else {
                        self.editor.submit()
                    };
                    self.execute_command(&cmd);
                }
            }
            TerminalKey::Backspace => {
                if self.editor.search_mode {
                    self.editor.search_query.pop();
                    self.search_history();
                } else {
                    self.editor.backspace();
                }
            }
            TerminalKey::Delete => self.editor.delete(),
            TerminalKey::Left => self.editor.move_left(),
            TerminalKey::Right => {
                if self.editor.cursor >= self.editor.buffer.len() {
                    // At end of line: accept suggestion (fish behavior)
                    if !self.editor.accept_suggestion() {
                        self.editor.move_right();
                    }
                } else {
                    self.editor.move_right();
                }
            }
            TerminalKey::Up => self.editor.history_prev(),
            TerminalKey::Down => self.editor.history_down(),
            TerminalKey::Home => self.editor.move_home(),
            TerminalKey::End => self.editor.move_end(),
            TerminalKey::Tab => self.editor.tab_complete(),
            TerminalKey::CtrlA => self.editor.move_home(),
            TerminalKey::CtrlE => self.editor.move_end(),
            TerminalKey::CtrlK => self.editor.kill_to_end(),
            TerminalKey::CtrlU => self.editor.kill_to_start(),
            TerminalKey::CtrlW => self.editor.kill_word(),
            TerminalKey::CtrlY => self.editor.yank(),
            TerminalKey::AltY => self.editor.yank_pop(),
            TerminalKey::CtrlZ => self.editor.undo(),
            TerminalKey::CtrlShiftZ => self.editor.redo(),
            TerminalKey::CtrlC => {
                // Print ^C and start new prompt
                self.write_output("^C\n");
                self.editor.cancel();
                self.prompt = shell::get_prompt();
            }
            TerminalKey::CtrlD => {
                if self.editor.buffer.is_empty() {
                    self.write_output("exit\n");
                }
            }
            TerminalKey::CtrlL => {
                // Clear screen
                self.vt.reset();
                self.scrollback.clear();
                self.scroll.offset = 0;
                self.scroll.total_scrollback = 0;
            }
            TerminalKey::CtrlR => self.editor.toggle_search(),
            TerminalKey::CtrlLeft => self.editor.move_word_left(),
            TerminalKey::CtrlRight => {
                if self.editor.cursor >= self.editor.buffer.len() {
                    self.editor.accept_suggestion_word();
                } else {
                    self.editor.move_word_right();
                }
            }
            TerminalKey::ShiftPgUp => self.scroll_up(self.vt.rows / 2),
            TerminalKey::ShiftPgDown => self.scroll_down(self.vt.rows / 2),
            TerminalKey::Escape => {
                // Vi mode: switch to normal mode
                if let EditingMode::Vi(ViMode::Insert) = self.editor.editing_mode {
                    self.editor.vi_escape_to_normal();
                } else {
                    self.editor.dismiss_completions();
                    self.editor.search_mode = false;
                }
            }
            TerminalKey::CtrlShiftC => {
                // Copy: get VT selection text → global clipboard
                if let Some(text) = self.vt.get_selection_text() {
                    self.vt.clipboard = text.clone();
                    crate::clipboard::copy_text(&text);
                }
            }
            TerminalKey::CtrlShiftV => {
                // Paste: prefer global clipboard, fall back to local
                let clip =
                    crate::clipboard::paste_text().unwrap_or_else(|| self.vt.clipboard.clone());
                if !clip.is_empty() {
                    self.editor.save_undo();
                    let before = String::from(&self.editor.buffer[..self.editor.cursor]);
                    let after = String::from(&self.editor.buffer[self.editor.cursor..]);
                    self.editor.buffer = alloc::format!("{}{}{}", before, clip, after);
                    self.editor.cursor += clip.len();
                    self.editor.update_highlight();
                    self.editor.update_suggestion();
                }
            }
            TerminalKey::F1 => {
                // Toggle vi/emacs mode
                self.editor.toggle_vi_mode();
                let mode_name = match self.editor.editing_mode {
                    EditingMode::Emacs => "emacs",
                    EditingMode::Vi(_) => "vi",
                };
                self.write_output(&alloc::format!("[{} mode]\n", mode_name));
            }
        }
    }

    /// Execute a command and display output
    fn execute_command(&mut self, cmd: &str) {
        // Echo the command with syntax highlighting to VT
        let prompt = self.prompt.clone();
        self.write_output(&prompt);
        self.write_output(cmd);
        self.write_output("\n");

        if cmd.is_empty() {
            self.prompt = shell::get_prompt();
            return;
        }

        // Execute through shell
        let result = shell::execute(cmd);

        // Display output
        if !result.output.is_empty() {
            self.write_output(&result.output);
            // Ensure output ends with newline
            if !result.output.ends_with('\n') {
                self.write_output("\n");
            }
        }

        // Update prompt (PWD may have changed)
        self.prompt = shell::get_prompt();
    }

    /// Reverse search through history (Ctrl+R)
    fn search_history(&mut self) {
        let history = shell::get_history();
        let query = self.editor.search_query.clone();

        if query.is_empty() {
            self.editor.search_result = None;
            return;
        }

        for (i, entry) in history.iter().enumerate().rev() {
            if entry.contains(&query) {
                self.editor.search_result = Some(i);
                self.editor.buffer = entry.clone();
                self.editor.cursor = self.editor.buffer.len();
                return;
            }
        }
        self.editor.search_result = None;
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, n: usize) {
        let max = self.scrollback.len();
        self.scroll.offset = (self.scroll.offset + n).min(max);
        self.dirty = true;
    }

    /// Scroll down by n lines
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll.offset = self.scroll.offset.saturating_sub(n);
        self.dirty = true;
    }

    /// Save current screen lines to scrollback before scroll
    pub fn save_to_scrollback(&mut self, line: Vec<Cell>) {
        self.scrollback.push(line);
        if self.scrollback.len() > self.max_scrollback {
            self.scrollback.remove(0);
        }
        self.scroll.total_scrollback = self.scrollback.len();
    }

    /// Tick the cursor blink animation
    /// Called every ~500ms from the redraw loop — simply toggle each time.
    pub fn tick_blink(&mut self) {
        self.blink_counter = self.blink_counter.wrapping_add(1);
        self.cursor_blink = !self.cursor_blink;
        self.dirty = true;
    }

    /// Convert VT color to pixel using the theme palette
    pub fn color_to_pixel(&self, color: &Color, is_bold: bool) -> Pixel {
        match color {
            Color::Default => self.theme.foreground,
            Color::Black => self.theme.palette[if is_bold { 8 } else { 0 }],
            Color::Red => self.theme.palette[if is_bold { 9 } else { 1 }],
            Color::Green => self.theme.palette[if is_bold { 10 } else { 2 }],
            Color::Yellow => self.theme.palette[if is_bold { 11 } else { 3 }],
            Color::Blue => self.theme.palette[if is_bold { 12 } else { 4 }],
            Color::Magenta => self.theme.palette[if is_bold { 13 } else { 5 }],
            Color::Cyan => self.theme.palette[if is_bold { 14 } else { 6 }],
            Color::White => self.theme.palette[if is_bold { 15 } else { 7 }],
            Color::BrightBlack => self.theme.palette[8],
            Color::BrightRed => self.theme.palette[9],
            Color::BrightGreen => self.theme.palette[10],
            Color::BrightYellow => self.theme.palette[11],
            Color::BrightBlue => self.theme.palette[12],
            Color::BrightMagenta => self.theme.palette[13],
            Color::BrightCyan => self.theme.palette[14],
            Color::BrightWhite => self.theme.palette[15],
            Color::Indexed(idx) => {
                let (r, g, b, _) = vterm::index_to_rgb(*idx);
                Pixel::rgb(r, g, b)
            }
            Color::Rgb(r, g, b) => Pixel::rgb(*r, *g, *b),
        }
    }

    /// Render the terminal onto a framebuffer region
    pub fn render(&mut self, fb: &mut FrameBuffer, rect: Rect) {
        // ── Safety-zone padding around terminal content ──
        // Provides visual breathing room between text and window edges,
        // similar to Alacritty/iTerm2 padding settings.
        const PAD_LEFT: i32 = 8;
        const PAD_RIGHT: i32 = 8;
        const PAD_TOP: i32 = 6;
        const PAD_BOTTOM: i32 = 4;

        let char_w = fonts::FONT_WIDTH as i32;
        let font_h = fonts::FONT_HEIGHT as i32; // actual glyph height
        let char_h = font_h + 2; // +2 for line spacing

        // Compute the inner content area after applying padding
        let inner_x = rect.x + PAD_LEFT;
        let inner_y = rect.y + PAD_TOP;
        let inner_w = (rect.width as i32 - PAD_LEFT - PAD_RIGHT).max(0);
        let inner_h = (rect.height as i32 - PAD_TOP - PAD_BOTTOM).max(0);

        let cols = (inner_w / char_w) as usize;
        let rows = (inner_h / char_h) as usize;
        let right_edge = inner_x + inner_w;

        if cols == 0 || rows == 0 {
            return;
        }

        // Resize VT if needed
        if self.vt.cols != cols || self.vt.rows != rows {
            self.vt.resize(cols, rows);
        }

        // Fill entire terminal background including padding zone
        fb.fill_rect(rect, self.theme.background);

        // ── Render VT screen buffer ──────────────────────────────
        let visible_rows = rows.min(self.vt.rows);
        for row in 0..visible_rows {
            let screen_row = if self.scroll.offset > 0 {
                // Show scrollback
                let scrollback_row =
                    self.scrollback.len() as isize - self.scroll.offset as isize + row as isize;
                if scrollback_row < 0 {
                    continue;
                }
                let srow = scrollback_row as usize;
                if srow < self.scrollback.len() {
                    // Draw from scrollback — skip entirely empty rows
                    let line = &self.scrollback[srow];
                    let max_col = line.len().min(cols);
                    for col in 0..max_col {
                        let cell = &line[col];
                        if cell.ch != ' ' && cell.ch != '\0' {
                            let fg = self.color_to_pixel(&cell.attr.fg, cell.attr.bold);
                            let x = inner_x + col as i32 * char_w;
                            if x + char_w > right_edge {
                                break;
                            }
                            let y = inner_y + row as i32 * char_h;
                            fonts::draw_char(fb, x, y, cell.ch, fg, 1);
                        }
                    }
                    continue;
                } else {
                    // Past scrollback, show from VT buffer
                    let vt_row = srow - self.scrollback.len();
                    if vt_row >= self.vt.rows {
                        continue;
                    }
                    vt_row
                }
            } else {
                row
            };

            if screen_row >= self.vt.rows {
                continue;
            }

            let max_col = cols.min(self.vt.cols);
            let y_base = inner_y + row as i32 * char_h;

            for col in 0..max_col {
                let cell = &self.vt.cells[screen_row][col];

                // Check if this cell is selected
                let selected = self.vt.is_selected(screen_row, col);

                // Skip completely empty/space cells (most cells in a terminal are empty)
                let is_space = cell.ch == ' ' || cell.ch == '\0';
                let has_bg = cell.attr.bg != Color::Default;
                if is_space && !has_bg && !selected {
                    continue;
                }

                let x = inner_x + col as i32 * char_w;

                // Draw selection highlight background
                if selected {
                    fb.fill_rect(
                        Rect::new(x, y_base, char_w as u32, char_h as u32),
                        self.theme.selection_bg,
                    );
                    if !is_space {
                        fonts::draw_char(fb, x, y_base, cell.ch, self.theme.selection_fg, 1);
                    }
                    continue;
                }

                // Draw cell background if not default
                if has_bg {
                    let bg = self.color_to_pixel(&cell.attr.bg, false);
                    fb.fill_rect(Rect::new(x, y_base, char_w as u32, char_h as u32), bg);
                }

                // Draw character
                if !is_space {
                    let mut fg = self.color_to_pixel(&cell.attr.fg, cell.attr.bold);
                    if cell.attr.dim {
                        fg = Pixel::new(fg.r / 2, fg.g / 2, fg.b / 2, fg.a);
                    }
                    if cell.attr.inverse {
                        let bg = self.color_to_pixel(&cell.attr.bg, false);
                        fb.fill_rect(Rect::new(x, y_base, char_w as u32, char_h as u32), fg);
                        fonts::draw_char(fb, x, y_base, cell.ch, bg, 1);
                    } else {
                        fonts::draw_char(fb, x, y_base, cell.ch, fg, 1);
                    }

                    // Underline
                    if cell.attr.underline {
                        fb.fill_rect(Rect::new(x, y_base + char_h - 1, char_w as u32, 1), fg);
                    }

                    // Strikethrough
                    if cell.attr.strikethrough {
                        fb.fill_rect(Rect::new(x, y_base + char_h / 2, char_w as u32, 1), fg);
                    }
                }

                // ── Clickable URL underline ──
                // Detect URLs in VT row text and draw a subtle underline
                {
                    let row_text: String = (0..max_col)
                        .map(|c| {
                            let ch = self.vt.cells[screen_row][c].ch;
                            if ch == '\0' { ' ' } else { ch }
                        })
                        .collect();
                    if clickable_urls::is_url_position(&row_text, screen_row, col) {
                        let url_color = Pixel::new(100, 149, 237, 255); // cornflower blue
                        fb.fill_rect(
                            Rect::new(x, y_base + char_h - 1, char_w as u32, 1),
                            url_color,
                        );
                    }
                }
            }
        }

        // ── Render Sixel graphics images ─────────────────────────
        // Composite sixel images stored by the VT emulator onto the framebuffer
        for image in &self.vt.sixel_images {
            let img_x_start = inner_x + image.col as i32 * char_w;
            let img_y_start = inner_y + image.row as i32 * char_h;
            for py in 0..image.height {
                for px in 0..image.width {
                    let fb_x = img_x_start + px as i32;
                    let fb_y = img_y_start + py as i32;
                    if fb_x < rect.x || fb_x >= rect.x + rect.width as i32 {
                        continue;
                    }
                    if fb_y < rect.y || fb_y >= rect.y + rect.height as i32 {
                        continue;
                    }
                    let offset = (py * image.width + px) * 4;
                    if offset + 3 < image.pixels.len() {
                        let r = image.pixels[offset];
                        let g = image.pixels[offset + 1];
                        let b = image.pixels[offset + 2];
                        let a = image.pixels[offset + 3];
                        if a > 0 {
                            let pixel = Pixel::new(r, g, b, a);
                            if a == 255 {
                                fb.set_pixel(fb_x as usize, fb_y as usize, pixel);
                            } else {
                                let bg = fb.get_pixel(fb_x as usize, fb_y as usize);
                                fb.set_pixel(fb_x as usize, fb_y as usize, pixel.blend_over(bg));
                            }
                        }
                    }
                }
            }
        }

        // ── Render Kitty graphics images (sorted by z-index) ─────
        // Kitty images are layered; we render them in z-index order
        if !self.vt.kitty_images.is_empty() {
            let mut sorted_indices: Vec<usize> = (0..self.vt.kitty_images.len()).collect();
            sorted_indices.sort_by_key(|&i| self.vt.kitty_images[i].z_index);
            for idx in sorted_indices {
                let image = &self.vt.kitty_images[idx];
                let img_x_start = inner_x + image.col as i32 * char_w;
                let img_y_start = inner_y + image.row as i32 * char_h;
                for py in 0..image.height {
                    for px in 0..image.width {
                        let fb_x = img_x_start + px as i32;
                        let fb_y = img_y_start + py as i32;
                        if fb_x < rect.x || fb_x >= rect.x + rect.width as i32 {
                            continue;
                        }
                        if fb_y < rect.y || fb_y >= rect.y + rect.height as i32 {
                            continue;
                        }
                        let offset = (py * image.width + px) * 4;
                        if offset + 3 < image.pixels.len() {
                            let r = image.pixels[offset];
                            let g = image.pixels[offset + 1];
                            let b = image.pixels[offset + 2];
                            let a = image.pixels[offset + 3];
                            if a > 0 {
                                let pixel = Pixel::new(r, g, b, a);
                                if a == 255 {
                                    fb.set_pixel(fb_x as usize, fb_y as usize, pixel);
                                } else {
                                    let bg = fb.get_pixel(fb_x as usize, fb_y as usize);
                                    fb.set_pixel(
                                        fb_x as usize,
                                        fb_y as usize,
                                        pixel.blend_over(bg),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Render the input line with multi-row wrapping ─────────
        // When the prompt + input text exceeds the terminal width (cols),
        // it wraps to subsequent rows, just like a real terminal emulator
        // (Alacritty, GNOME Terminal, etc.).
        if self.input_mode && self.scroll.offset == 0 {
            let prompt_row = self.vt.cursor_row;
            let bottom_edge = inner_y + inner_h;

            // Clear VT cells and background at the prompt row to prevent
            // ghost content from the VT cell grid bleeding through. The input
            // line renderer fully owns the row(s) starting at prompt_row.
            let total_input_chars = strip_ansi_visible_len(&self.prompt)
                + self.editor.buffer.len()
                + self
                    .editor
                    .suggestion
                    .as_ref()
                    .map_or(0, |s| s.len().saturating_sub(self.editor.buffer.len()));
            let input_rows = if cols > 0 {
                total_input_chars.div_ceil(cols)
            } else {
                1
            };
            for r in 0..input_rows {
                let clear_row = prompt_row + r;
                if clear_row < self.vt.rows {
                    // Clear VT cells on this row
                    for col in 0..self.vt.cols.min(cols) {
                        self.vt.cells[clear_row][col] = vterm::Cell::default();
                    }
                    // Redraw the background for this row
                    let clear_y = inner_y + clear_row as i32 * char_h;
                    if clear_y + char_h <= bottom_edge {
                        fb.fill_rect(
                            Rect::new(inner_x, clear_y, inner_w as u32, char_h as u32),
                            self.theme.background,
                        );
                    }
                }
            }

            // Helper: convert a linear character position to (row, col) on the grid
            let pos_to_rc = |linear_col: usize| -> (usize, usize) {
                if cols == 0 {
                    return (0, 0);
                }
                (linear_col / cols, linear_col % cols)
            };

            // Helper: draw a character at a (row, col) grid position relative to prompt_row
            let draw_at =
                |fb: &mut FrameBuffer, grid_col: usize, grid_row: usize, ch: char, color: Pixel| {
                    let py = inner_y + (prompt_row + grid_row) as i32 * char_h;
                    if py + char_h > bottom_edge {
                        return;
                    }
                    let px = inner_x + grid_col as i32 * char_w;
                    if px + char_w > right_edge {
                        return;
                    }
                    fonts::draw_char(fb, px, py, ch, color, 1);
                };

            // ── 1. Render the prompt (ANSI-colored) with wrapping ──
            let prompt_text = &self.prompt;
            // Compute visible length by stripping ANSI escape sequences from the
            // actual prompt string (more robust than relying on get_prompt_plain)
            let prompt_visible_len = strip_ansi_visible_len(prompt_text);

            {
                let mut linear_col = 0usize;
                let mut in_esc = false;
                let mut fg_color = self.theme.foreground;
                let mut is_bold = false;

                let bytes = prompt_text.as_bytes();
                let mut bi = 0;
                while bi < bytes.len() {
                    if bytes[bi] == 0x1b {
                        in_esc = true;
                        bi += 1;
                        if bi < bytes.len() && bytes[bi] == b'[' {
                            bi += 1;
                            let mut params = alloc::vec::Vec::new();
                            let mut num = 0u32;
                            let mut has_num = false;
                            while bi < bytes.len() {
                                let b = bytes[bi];
                                if b.is_ascii_digit() {
                                    num = num * 10 + (b - b'0') as u32;
                                    has_num = true;
                                    bi += 1;
                                } else if b == b';' {
                                    params.push(if has_num { num } else { 0 });
                                    num = 0;
                                    has_num = false;
                                    bi += 1;
                                } else {
                                    if has_num {
                                        params.push(num);
                                    }
                                    if b == b'm' {
                                        if params.is_empty() {
                                            params.push(0);
                                        }
                                        let mut pi = 0;
                                        while pi < params.len() {
                                            match params[pi] {
                                                0 => {
                                                    fg_color = self.theme.foreground;
                                                    is_bold = false;
                                                }
                                                1 => {
                                                    is_bold = true;
                                                }
                                                30 => fg_color = self.theme.palette[0],
                                                31 => {
                                                    fg_color = self.theme.palette
                                                        [if is_bold { 9 } else { 1 }]
                                                }
                                                32 => {
                                                    fg_color = self.theme.palette
                                                        [if is_bold { 10 } else { 2 }]
                                                }
                                                33 => {
                                                    fg_color = self.theme.palette
                                                        [if is_bold { 11 } else { 3 }]
                                                }
                                                34 => {
                                                    fg_color = self.theme.palette
                                                        [if is_bold { 12 } else { 4 }]
                                                }
                                                35 => {
                                                    fg_color = self.theme.palette
                                                        [if is_bold { 13 } else { 5 }]
                                                }
                                                36 => {
                                                    fg_color = self.theme.palette
                                                        [if is_bold { 14 } else { 6 }]
                                                }
                                                37 => {
                                                    fg_color = self.theme.palette
                                                        [if is_bold { 15 } else { 7 }]
                                                }
                                                _ => {}
                                            }
                                            pi += 1;
                                        }
                                    }
                                    bi += 1;
                                    break;
                                }
                            }
                        }
                        in_esc = false;
                        continue;
                    }

                    if !in_esc {
                        let ch = bytes[bi] as char;
                        let (gr, gc) = pos_to_rc(linear_col);
                        draw_at(fb, gc, gr, ch, fg_color);
                        linear_col += 1;
                    }
                    bi += 1;
                }
            }

            // ── 2. Render editor buffer with syntax highlighting + wrapping ──
            let input_linear_start = prompt_visible_len;
            if !self.editor.tokens.is_empty() {
                for token in &self.editor.tokens {
                    let color = token_color(token.kind, &self.theme);
                    for (j, ch) in token.text.chars().enumerate() {
                        let char_pos = token.start + j;
                        let linear = input_linear_start + char_pos;
                        let (gr, gc) = pos_to_rc(linear);
                        draw_at(fb, gc, gr, ch, color);
                    }
                }
            } else {
                for (i, ch) in self.editor.buffer.chars().enumerate() {
                    let linear = input_linear_start + i;
                    let (gr, gc) = pos_to_rc(linear);
                    draw_at(fb, gc, gr, ch, self.theme.foreground);
                }
            }

            // ── 3. Autosuggestion ghost text (wrapping) ──
            if let Some(ref suggestion) = self.editor.suggestion {
                if suggestion.len() > self.editor.buffer.len() {
                    let ghost = &suggestion[self.editor.buffer.len()..];
                    let ghost_linear_start = input_linear_start + self.editor.buffer.len();
                    for (i, ch) in ghost.chars().enumerate() {
                        let linear = ghost_linear_start + i;
                        let (gr, gc) = pos_to_rc(linear);
                        draw_at(fb, gc, gr, ch, self.theme.suggestion_fg);
                    }
                }
            }

            // ── 4. Cursor (wrapping-aware) ──
            if self.cursor_blink || !self.vt.cursor_visible {
                let cursor_linear = input_linear_start + self.editor.cursor;
                let (cr, cc) = pos_to_rc(cursor_linear);
                let cursor_px = inner_x + cc as i32 * char_w;
                let cursor_py = inner_y + (prompt_row + cr) as i32 * char_h;
                if cursor_px + char_w <= right_edge && cursor_py + font_h <= bottom_edge {
                    match self.editor.cursor_style {
                        CursorStyle::Block => {
                            fb.fill_rect(
                                Rect::new(cursor_px, cursor_py, char_w as u32, font_h as u32),
                                self.theme.cursor,
                            );
                            if self.editor.cursor < self.editor.buffer.len() {
                                let ch = self.editor.buffer.as_bytes()[self.editor.cursor] as char;
                                fonts::draw_char(
                                    fb,
                                    cursor_px,
                                    cursor_py,
                                    ch,
                                    self.theme.cursor_text,
                                    1,
                                );
                            }
                        }
                        CursorStyle::Bar => {
                            fb.fill_rect(
                                Rect::new(cursor_px, cursor_py, 2, font_h as u32),
                                self.theme.cursor,
                            );
                        }
                        CursorStyle::Underline => {
                            fb.fill_rect(
                                Rect::new(cursor_px, cursor_py + font_h - 2, char_w as u32, 2),
                                self.theme.cursor,
                            );
                        }
                    }
                }
            }

            // ── 5. Completion menu (positioned after last wrapped row) ──
            if self.editor.showing_completions && !self.editor.completions.is_empty() {
                let total_linear = input_linear_start + self.editor.buffer.len();
                let (last_row, _) = pos_to_rc(total_linear);
                let menu_y = inner_y + (prompt_row + last_row + 1) as i32 * char_h;
                let menu_x = inner_x + prompt_visible_len as i32 * char_w;
                self.render_completion_menu(fb, rect, menu_x, menu_y, char_w, char_h);
            }

            // ── 6. Search bar ──
            if self.editor.search_mode {
                self.render_search_bar(fb, rect, char_w, char_h);
            }
        }

        // ── Render scrollbar ─────────────────────────────────────
        if !self.scrollback.is_empty() {
            self.render_scrollbar(fb, rect);
        }

        // ── Render tmux pane borders and status line ─────────────
        // When running in tmux mode, draw pane separators and a status bar
        if let Some(session_id) = self.mux_session_id {
            self.render_mux_overlay(fb, rect, session_id, char_w, char_h);
        }

        self.dirty = false;
    }

    /// Handle a Ctrl+Click on the terminal content area.
    /// Converts pixel coordinates to cell coordinates and checks for URLs.
    pub fn handle_ctrl_click(&self, click_x: i32, click_y: i32, rect: Rect) {
        const PAD_LEFT: i32 = 8;
        const PAD_TOP: i32 = 6;
        let char_w = fonts::FONT_WIDTH as i32;
        let font_h = fonts::FONT_HEIGHT as i32;
        let char_h = font_h + 2;

        let inner_x = rect.x + PAD_LEFT;
        let inner_y = rect.y + PAD_TOP;

        // Convert pixel to cell coordinates
        let col = ((click_x - inner_x) / char_w) as usize;
        let row = ((click_y - inner_y) / char_h) as usize;

        if row >= self.vt.rows || col >= self.vt.cols {
            return;
        }

        // Build the row text for URL detection
        let max_col = self.vt.cols;
        let row_text: alloc::string::String = (0..max_col)
            .map(|c| {
                let ch = self.vt.cells[row][c].ch;
                if ch == '\0' { ' ' } else { ch }
            })
            .collect();

        if let Some(url) = clickable_urls::url_at_position(&row_text, row, col) {
            clickable_urls::open_url(&url.url);
        }
    }

    /// Render tmux multiplexer overlay: pane borders and status bar
    fn render_mux_overlay(
        &self,
        fb: &mut FrameBuffer,
        rect: Rect,
        session_id: u32,
        char_w: i32,
        char_h: i32,
    ) {
        let sessions = crate::tmux::MUX_SESSIONS.lock();
        let session = match sessions.get(&session_id) {
            Some(s) => s,
            None => return,
        };

        if session.detached {
            return;
        }

        let border_color = Pixel::new(100, 100, 120, 255);
        let status_bg = Pixel::new(30, 35, 50, 255);
        let status_fg = Pixel::new(180, 190, 210, 255);
        let active_fg = Pixel::new(130, 230, 130, 255);

        // Draw pane borders for the active window
        if let Some(window) = session.windows.get(session.active_window) {
            if window.panes.len() > 1 {
                for pane in &window.panes {
                    // Draw horizontal border above pane (if not at row 0)
                    if pane.row > 0 {
                        let border_y = rect.y + (pane.row as i32 - 1) * char_h + char_h / 2;
                        let border_x_start = rect.x + pane.col as i32 * char_w;
                        let border_width = pane.width as i32 * char_w;
                        if border_y >= rect.y && border_y < rect.y + rect.height as i32 {
                            fb.fill_rect(
                                Rect::new(border_x_start, border_y, border_width as u32, 1),
                                border_color,
                            );
                        }
                    }
                    // Draw vertical border left of pane (if not at col 0)
                    if pane.col > 0 {
                        let border_x = rect.x + (pane.col as i32 - 1) * char_w + char_w / 2;
                        let border_y_start = rect.y + pane.row as i32 * char_h;
                        let border_height = pane.height as i32 * char_h;
                        if border_x >= rect.x && border_x < rect.x + rect.width as i32 {
                            fb.fill_rect(
                                Rect::new(border_x, border_y_start, 1, border_height as u32),
                                border_color,
                            );
                        }
                    }

                    // Draw active pane indicator (bright border highlight)
                    if pane.active {
                        let highlight = Pixel::new(100, 200, 255, 255);
                        // Top edge
                        let top_y = rect.y + pane.row as i32 * char_h;
                        let left_x = rect.x + pane.col as i32 * char_w;
                        let w = (pane.width as i32 * char_w) as u32;
                        let h = (pane.height as i32 * char_h) as u32;
                        fb.fill_rect(Rect::new(left_x, top_y, w, 1), highlight);
                        // Bottom edge
                        fb.fill_rect(Rect::new(left_x, top_y + h as i32 - 1, w, 1), highlight);
                        // Left edge
                        fb.fill_rect(Rect::new(left_x, top_y, 1, h), highlight);
                        // Right edge
                        fb.fill_rect(Rect::new(left_x + w as i32 - 1, top_y, 1, h), highlight);
                    }
                }
            }
        }

        // Draw tmux status bar at the bottom of the terminal area
        let status_height = char_h;
        let status_y = rect.y + rect.height as i32 - status_height;
        fb.fill_rect(
            Rect::new(rect.x, status_y, rect.width, status_height as u32),
            status_bg,
        );

        // Status bar content: [session_name] window_list
        let mut x_pos = rect.x + 4;
        let session_label = alloc::format!("[{}] ", session.name);
        for ch in session_label.chars() {
            if x_pos + char_w <= rect.x + rect.width as i32 {
                fonts::draw_char(fb, x_pos, status_y + 1, ch, active_fg, 1);
                x_pos += char_w;
            }
        }

        // Draw window list
        for (i, window) in session.windows.iter().enumerate() {
            let is_active = i == session.active_window;
            let label = alloc::format!("{}:{} ", i, window.name);
            let color = if is_active { active_fg } else { status_fg };
            for ch in label.chars() {
                if x_pos + char_w <= rect.x + rect.width as i32 {
                    fonts::draw_char(fb, x_pos, status_y + 1, ch, color, 1);
                    x_pos += char_w;
                }
            }
        }
    }

    /// Render the tab completion popup menu
    fn render_completion_menu(
        &self,
        fb: &mut FrameBuffer,
        term_rect: Rect,
        menu_x: i32,
        menu_y: i32,
        char_w: i32,
        char_h: i32,
    ) {
        let max_visible = 8;
        let count = self.editor.completions.len().min(max_visible);
        let menu_width = 300i32;
        let menu_height = count as i32 * (char_h + 2) + 4;

        // Background with border
        let menu_bg = Pixel::rgb(36, 37, 48);
        let menu_border = Pixel::rgb(68, 71, 90);
        let selected_bg = Pixel::rgb(55, 58, 75);

        // Clamp menu position to terminal bounds
        let mx = menu_x.min(term_rect.x + term_rect.width as i32 - menu_width);
        let my = if menu_y + menu_height > term_rect.y + term_rect.height as i32 {
            menu_y - menu_height - char_h // Show above if no room below
        } else {
            menu_y
        };

        fb.fill_rect(
            Rect::new(mx, my, menu_width as u32, menu_height as u32),
            menu_bg,
        );
        fb.draw_rect(
            Rect::new(mx, my, menu_width as u32, menu_height as u32),
            menu_border,
            1,
        );

        let selected = self.editor.completion_index.unwrap_or(0);

        for (i, completion) in self.editor.completions.iter().take(count).enumerate() {
            let iy = my + 2 + i as i32 * (char_h + 2);

            // Highlight selected item
            if i == selected {
                fb.fill_rect(
                    Rect::new(mx + 1, iy, menu_width as u32 - 2, (char_h + 2) as u32),
                    selected_bg,
                );
            }

            // Icon based on kind
            let icon_color = match completion.kind {
                CompletionKind::Command => self.theme.command_fg,
                CompletionKind::File => self.theme.foreground,
                CompletionKind::Directory => self.theme.path_fg,
                CompletionKind::Variable => self.theme.variable_fg,
                CompletionKind::Flag => self.theme.foreground,
            };

            let icon = match completion.kind {
                CompletionKind::Command => '>',
                CompletionKind::File => '-',
                CompletionKind::Directory => '/',
                CompletionKind::Variable => '$',
                CompletionKind::Flag => '-',
            };

            fonts::draw_char(fb, mx + 6, iy + 1, icon, icon_color, 1);

            // Completion text
            let display = if completion.display.len() > 25 {
                alloc::format!("{}...", &completion.display[..22])
            } else {
                completion.display.clone()
            };
            fonts::draw_string(fb, mx + 18, iy + 1, &display, self.theme.foreground, 1);

            // Description (right-aligned)
            if !completion.description.is_empty() {
                let desc = if completion.description.len() > 15 {
                    alloc::format!("{}...", &completion.description[..12])
                } else {
                    completion.description.clone()
                };
                let desc_x = mx + menu_width - desc.len() as i32 * char_w - 6;
                fonts::draw_string(fb, desc_x, iy + 1, &desc, self.theme.comment_fg, 1);
            }
        }

        // Show count if more items exist
        if self.editor.completions.len() > max_visible {
            let more = alloc::format!("... +{} more", self.editor.completions.len() - max_visible);
            fonts::draw_string(
                fb,
                mx + 6,
                my + menu_height - char_h,
                &more,
                self.theme.comment_fg,
                1,
            );
        }
    }

    /// Render the Ctrl+R search bar
    fn render_search_bar(&self, fb: &mut FrameBuffer, rect: Rect, char_w: i32, char_h: i32) {
        let bar_h = char_h + 8;
        let bar_y = rect.y + rect.height as i32 - bar_h;
        let bar_bg = Pixel::rgb(36, 37, 48);
        let border = Pixel::rgb(82, 139, 255);

        fb.fill_rect(Rect::new(rect.x, bar_y, rect.width, bar_h as u32), bar_bg);
        fb.fill_rect(Rect::new(rect.x, bar_y, rect.width, 1), border);

        let label = "reverse-i-search: ";
        fonts::draw_string(
            fb,
            rect.x + 8,
            bar_y + 4,
            label,
            self.theme.palette[3], // Yellow
            1,
        );
        fonts::draw_string(
            fb,
            rect.x + 8 + label.len() as i32 * char_w,
            bar_y + 4,
            &self.editor.search_query,
            self.theme.foreground,
            1,
        );
    }

    /// Render a scrollbar on the right edge
    fn render_scrollbar(&self, fb: &mut FrameBuffer, rect: Rect) {
        let scrollbar_w = 6u32;
        let sb_x = rect.x + rect.width as i32 - scrollbar_w as i32;
        let sb_h = rect.height;

        // Track
        fb.fill_rect(
            Rect::new(sb_x, rect.y, scrollbar_w, sb_h),
            self.theme.scrollbar_bg,
        );

        // Thumb
        let total = self.scrollback.len() + self.vt.rows;
        if total > self.vt.rows {
            let thumb_h = ((self.vt.rows as u32 * sb_h) / total as u32).max(20);
            let thumb_offset = ((self.scrollback.len() - self.scroll.offset) as u32
                * (sb_h - thumb_h))
                / total as u32;
            fb.fill_rect(
                Rect::new(sb_x, rect.y + thumb_offset as i32, scrollbar_w, thumb_h),
                self.theme.scrollbar_thumb,
            );
        }
    }
}
