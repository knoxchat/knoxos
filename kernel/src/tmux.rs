/// Terminal Multiplexer — screen/tmux-style terminal multiplexing (P8.10)
/// Provides multiple virtual terminals within a single terminal window,
/// with split panes, detach/reattach, and session persistence.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;
use crate::vterm::VtEmulator;

/// A multiplexer pane — a single terminal within a window
pub struct MuxPane {
    /// Unique pane ID
    pub id: u32,
    /// VT emulator for this pane
    pub vterm: VtEmulator,
    /// PTY ID associated with this pane
    pub pty_id: u32,
    /// Pane title
    pub title: String,
    /// Position and size within the window (row, col, width, height in cells)
    pub row: usize,
    pub col: usize,
    pub width: usize,
    pub height: usize,
    /// Whether this pane is active (has focus)
    pub active: bool,
}

/// A multiplexer window — contains one or more panes
pub struct MuxWindow {
    /// Window ID
    pub id: u32,
    /// Window name
    pub name: String,
    /// Panes in this window
    pub panes: Vec<MuxPane>,
    /// Active pane index
    pub active_pane: usize,
    /// Next pane ID
    next_pane_id: u32,
}

impl MuxWindow {
    pub fn new(id: u32, name: &str, cols: usize, rows: usize) -> Self {
        let pty_id = crate::tty::alloc_pty();
        let vterm = VtEmulator::new(cols, rows);
        let pane = MuxPane {
            id: 0,
            vterm,
            pty_id,
            title: String::from(name),
            row: 0,
            col: 0,
            width: cols,
            height: rows,
            active: true,
        };
        Self {
            id,
            name: String::from(name),
            panes: vec![pane],
            active_pane: 0,
            next_pane_id: 1,
        }
    }

    /// Split the active pane horizontally (top/bottom)
    pub fn split_horizontal(&mut self) {
        if let Some(pane) = self.panes.get(self.active_pane) {
            let old_height = pane.height;
            if old_height < 4 {
                return; // Too small to split
            }
            let top_height = old_height / 2;
            let bottom_height = old_height - top_height - 1; // -1 for separator
            let col = pane.col;
            let width = pane.width;
            let row = pane.row;

            // Resize existing pane (top half)
            self.panes[self.active_pane].height = top_height;
            self.panes[self.active_pane].vterm = VtEmulator::new(width, top_height);

            // Create new pane (bottom half)
            let pty_id = crate::tty::alloc_pty();
            let new_pane = MuxPane {
                id: self.next_pane_id,
                vterm: VtEmulator::new(width, bottom_height),
                pty_id,
                title: String::from(""),
                row: row + top_height + 1,
                col,
                width,
                height: bottom_height,
                active: false,
            };
            self.next_pane_id += 1;
            self.panes.push(new_pane);
        }
    }

    /// Split the active pane vertically (left/right)
    pub fn split_vertical(&mut self) {
        if let Some(pane) = self.panes.get(self.active_pane) {
            let old_width = pane.width;
            if old_width < 4 {
                return;
            }
            let left_width = old_width / 2;
            let right_width = old_width - left_width - 1; // -1 for separator
            let row = pane.row;
            let height = pane.height;
            let col = pane.col;

            // Resize existing pane (left half)
            self.panes[self.active_pane].width = left_width;
            self.panes[self.active_pane].vterm = VtEmulator::new(left_width, height);

            // Create new pane (right half)
            let pty_id = crate::tty::alloc_pty();
            let new_pane = MuxPane {
                id: self.next_pane_id,
                vterm: VtEmulator::new(right_width, height),
                pty_id,
                title: String::from(""),
                row,
                col: col + left_width + 1,
                width: right_width,
                height,
                active: false,
            };
            self.next_pane_id += 1;
            self.panes.push(new_pane);
        }
    }

    /// Cycle focus to next pane
    pub fn next_pane(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        self.panes[self.active_pane].active = false;
        self.active_pane = (self.active_pane + 1) % self.panes.len();
        self.panes[self.active_pane].active = true;
    }

    /// Cycle focus to previous pane
    pub fn prev_pane(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        self.panes[self.active_pane].active = false;
        self.active_pane = if self.active_pane == 0 {
            self.panes.len() - 1
        } else {
            self.active_pane - 1
        };
        self.panes[self.active_pane].active = true;
    }

    /// Close the active pane
    pub fn close_pane(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return false; // Can't close the last pane
        }
        self.panes.remove(self.active_pane);
        if self.active_pane >= self.panes.len() {
            self.active_pane = self.panes.len() - 1;
        }
        self.panes[self.active_pane].active = true;
        true
    }

    /// Feed input to the active pane
    pub fn input(&mut self, data: &[u8]) {
        if let Some(pane) = self.panes.get_mut(self.active_pane) {
            for &byte in data {
                crate::tty::tty_input(pane.pty_id, byte);
            }
        }
    }

    /// Process output from all panes
    pub fn process_output(&mut self) {
        for pane in &mut self.panes {
            let mut buf = [0u8; 4096];
            let n = crate::tty::tty_read(pane.pty_id, &mut buf);
            if n > 0 {
                for &byte in &buf[..n] {
                    pane.vterm.process_byte(byte);
                }
            }
        }
    }
}

/// A multiplexer session — contains one or more windows
pub struct MuxSession {
    /// Session ID
    pub id: u32,
    /// Session name
    pub name: String,
    /// Windows in this session
    pub windows: Vec<MuxWindow>,
    /// Active window index
    pub active_window: usize,
    /// Next window ID
    next_window_id: u32,
    /// Whether session is detached
    pub detached: bool,
    /// Terminal dimensions
    pub cols: usize,
    pub rows: usize,
}

impl MuxSession {
    pub fn new(id: u32, name: &str, cols: usize, rows: usize) -> Self {
        let window = MuxWindow::new(0, "0", cols, rows);
        Self {
            id,
            name: String::from(name),
            windows: vec![window],
            active_window: 0,
            next_window_id: 1,
            detached: false,
            cols,
            rows,
        }
    }

    /// Create a new window
    pub fn new_window(&mut self, name: &str) {
        let id = self.next_window_id;
        self.next_window_id += 1;
        let window = MuxWindow::new(id, name, self.cols, self.rows);
        self.windows.push(window);
        self.active_window = self.windows.len() - 1;
    }

    /// Switch to next window
    pub fn next_window(&mut self) {
        if !self.windows.is_empty() {
            self.active_window = (self.active_window + 1) % self.windows.len();
        }
    }

    /// Switch to previous window
    pub fn prev_window(&mut self) {
        if !self.windows.is_empty() {
            self.active_window = if self.active_window == 0 {
                self.windows.len() - 1
            } else {
                self.active_window - 1
            };
        }
    }

    /// Close the active window
    pub fn close_window(&mut self) -> bool {
        if self.windows.len() <= 1 {
            return false;
        }
        self.windows.remove(self.active_window);
        if self.active_window >= self.windows.len() {
            self.active_window = self.windows.len() - 1;
        }
        true
    }

    /// Detach the session (keeps running in background)
    pub fn detach(&mut self) {
        self.detached = true;
        serial_println!("[tmux] Session '{}' detached", self.name);
    }

    /// Reattach the session
    pub fn attach(&mut self) {
        self.detached = false;
        serial_println!("[tmux] Session '{}' attached", self.name);
    }

    /// Get the active window
    pub fn active_window_mut(&mut self) -> Option<&mut MuxWindow> {
        self.windows.get_mut(self.active_window)
    }

    /// Process a tmux command (Ctrl+B prefix followed by key)
    pub fn handle_command(&mut self, key: char) {
        match key {
            // Window management
            'c' => {
                let name = alloc::format!("{}", self.next_window_id);
                self.new_window(&name);
            }
            'n' => self.next_window(),
            'p' => self.prev_window(),
            '&' => {
                let _ = self.close_window();
            }
            // Pane management
            '"' => {
                if let Some(w) = self.windows.get_mut(self.active_window) {
                    w.split_horizontal();
                }
            }
            '%' => {
                if let Some(w) = self.windows.get_mut(self.active_window) {
                    w.split_vertical();
                }
            }
            'o' => {
                if let Some(w) = self.windows.get_mut(self.active_window) {
                    w.next_pane();
                }
            }
            'x' => {
                if let Some(w) = self.windows.get_mut(self.active_window) {
                    let _ = w.close_pane();
                }
            }
            // Session management
            'd' => self.detach(),
            // Window selection by number
            '0'..='9' => {
                let idx = (key as usize) - ('0' as usize);
                if idx < self.windows.len() {
                    self.active_window = idx;
                }
            }
            _ => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Global multiplexer state
// ═══════════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// All multiplexer sessions
    pub static ref MUX_SESSIONS: Mutex<BTreeMap<u32, MuxSession>> = Mutex::new(BTreeMap::new());
}

static NEXT_SESSION_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

/// Create a new tmux session
pub fn new_session(name: &str) -> u32 {
    let id = NEXT_SESSION_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let session = MuxSession::new(id, name, 80, 25);
    MUX_SESSIONS.lock().insert(id, session);
    serial_println!("[tmux] Created session '{}' (id={})", name, id);
    id
}

/// List all sessions
pub fn list_sessions() -> Vec<(u32, String, bool, usize)> {
    let sessions = MUX_SESSIONS.lock();
    sessions
        .values()
        .map(|s| (s.id, s.name.clone(), s.detached, s.windows.len()))
        .collect()
}

/// Attach to a session by ID
pub fn attach_session(id: u32) -> bool {
    let mut sessions = MUX_SESSIONS.lock();
    if let Some(session) = sessions.get_mut(&id) {
        session.attach();
        true
    } else {
        false
    }
}

/// Detach a session by ID
pub fn detach_session(id: u32) -> bool {
    let mut sessions = MUX_SESSIONS.lock();
    if let Some(session) = sessions.get_mut(&id) {
        session.detach();
        true
    } else {
        false
    }
}

/// Kill a session by ID
pub fn kill_session(id: u32) -> bool {
    MUX_SESSIONS.lock().remove(&id).is_some()
}

/// Initialize the tmux subsystem
pub fn init() {
    // Force lazy init
    let _ = MUX_SESSIONS.lock();
    serial_println!("[KnoxOS] Terminal multiplexer (tmux) subsystem initialized");
}

/// Get the VT cell content for a specific pane in the active window of a session
/// Returns (row_offset, col_offset, rows, cols, cells) for rendering
pub fn get_pane_content(
    session_id: u32,
) -> Vec<(usize, usize, usize, usize, Vec<Vec<crate::vterm::Cell>>)> {
    let mut sessions = MUX_SESSIONS.lock();
    let session = match sessions.get_mut(&session_id) {
        Some(s) => s,
        None => return Vec::new(),
    };

    if session.detached {
        return Vec::new();
    }

    // Process output for all panes in the active window
    if let Some(window) = session.windows.get_mut(session.active_window) {
        window.process_output();
    }

    let window = match session.windows.get(session.active_window) {
        Some(w) => w,
        None => return Vec::new(),
    };

    let mut pane_contents = Vec::new();
    for pane in &window.panes {
        let cells = pane.vterm.cells.clone();
        pane_contents.push((pane.row, pane.col, pane.height, pane.width, cells));
    }
    pane_contents
}
