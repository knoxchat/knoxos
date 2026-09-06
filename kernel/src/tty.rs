/// TTY/PTY Subsystem - Terminal device management
/// Implements Linux-compatible terminal I/O with line discipline
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Terminal I/O flags (Linux termios-compatible subset)
#[derive(Debug, Clone, Copy)]
pub struct Termios {
    /// Input flags
    pub c_iflag: u32,
    /// Output flags
    pub c_oflag: u32,
    /// Control flags
    pub c_cflag: u32,
    /// Local flags
    pub c_lflag: u32,
    /// Line discipline
    pub c_line: u8,
    /// Special characters
    pub c_cc: [u8; 32],
}

impl Default for Termios {
    fn default() -> Self {
        let mut cc = [0u8; 32];
        cc[VINTR] = 3; // Ctrl+C
        cc[VQUIT] = 28; // Ctrl+backslash
        cc[VERASE] = 127; // Backspace/Delete
        cc[VKILL] = 21; // Ctrl+U
        cc[VEOF] = 4; // Ctrl+D
        cc[VMIN] = 1;
        cc[VTIME] = 0;
        cc[VSUSP] = 26; // Ctrl+Z
        cc[VSTART] = 17; // Ctrl+Q
        cc[VSTOP] = 19; // Ctrl+S
        cc[VWERASE] = 23; // Ctrl+W (word erase)
        cc[VLNEXT] = 22; // Ctrl+V (literal next)
        cc[VREPRINT] = 18; // Ctrl+R (reprint)

        Self {
            c_iflag: ICRNL | IXON,
            c_oflag: OPOST | ONLCR,
            c_cflag: CS8 | CREAD | CLOCAL,
            c_lflag: ISIG | ICANON | ECHO | ECHOE | ECHOK | IEXTEN | IUTF8,
            c_line: 0,
            c_cc: cc,
        }
    }
}

// Input flags
pub const ICRNL: u32 = 0o000400; // Map CR to NL on input
pub const IXON: u32 = 0o002000; // Enable XON/XOFF flow control
pub const IGNBRK: u32 = 0o000001; // Ignore break

// Output flags
pub const OPOST: u32 = 0o000001; // Post-process output
pub const ONLCR: u32 = 0o000004; // Map NL to CR-NL on output

// Control flags
pub const CS8: u32 = 0o000060; // 8-bit characters
pub const CREAD: u32 = 0o000200; // Enable receiver
pub const CLOCAL: u32 = 0o004000; // Ignore modem status

// Local flags
pub const ISIG: u32 = 0o000001; // Enable signals (INTR, QUIT, SUSP)
pub const ICANON: u32 = 0o000002; // Canonical mode (line-buffered)
pub const ECHO: u32 = 0o000010; // Echo input characters
pub const ECHOE: u32 = 0o000020; // Echo erase as BS-SP-BS
pub const ECHOK: u32 = 0o000040; // Echo NL after KILL
pub const IEXTEN: u32 = 0o100000; // Extended input processing
pub const IUTF8: u32 = 0o200000; // UTF-8 aware line editing

// Control character indices
pub const VINTR: usize = 0;
pub const VQUIT: usize = 1;
pub const VERASE: usize = 2;
pub const VKILL: usize = 3;
pub const VEOF: usize = 4;
pub const VTIME: usize = 5;
pub const VMIN: usize = 6;
pub const VSUSP: usize = 10;
pub const VSTART: usize = 8;
pub const VSTOP: usize = 9;
pub const VWERASE: usize = 14;
pub const VLNEXT: usize = 15;
pub const VREPRINT: usize = 12;

/// A TTY device
pub struct Tty {
    /// TTY number
    pub id: u32,
    /// Terminal name
    pub name: String,
    /// Terminal settings
    pub termios: Termios,
    /// Input buffer (keyboard -> process)
    pub input_buf: VecDeque<u8>,
    /// Output buffer (process -> screen)
    pub output_buf: VecDeque<u8>,
    /// Line edit buffer (for canonical mode)
    pub line_buf: Vec<u8>,
    /// Foreground process group
    pub fg_pgid: u32,
    /// Session ID
    pub session: u32,
    /// Window size
    pub winsize: WinSize,
    /// Whether the TTY is open
    pub is_open: bool,
    /// Column position (for tab stops)
    pub column: u32,
    /// XON/XOFF: output stopped by Ctrl+S
    pub output_stopped: bool,
    /// VLNEXT: next character is literal (Ctrl+V)
    pub lnext: bool,
}

/// Terminal window size
#[derive(Debug, Clone, Copy)]
pub struct WinSize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

impl Default for WinSize {
    fn default() -> Self {
        Self {
            ws_row: 25,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}

impl Tty {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: String::from(name),
            termios: Termios::default(),
            input_buf: VecDeque::new(),
            output_buf: VecDeque::new(),
            line_buf: Vec::new(),
            fg_pgid: 0,
            session: 0,
            winsize: WinSize::default(),
            is_open: false,
            column: 0,
            output_stopped: false,
            lnext: false,
        }
    }

    /// Process a character from keyboard input
    pub fn input_char(&mut self, ch: u8) {
        let canonical = self.termios.c_lflag & ICANON != 0;
        let echo = self.termios.c_lflag & ECHO != 0;
        let isig = self.termios.c_lflag & ISIG != 0;
        let iexten = self.termios.c_lflag & IEXTEN != 0;
        let ixon = self.termios.c_iflag & IXON != 0;

        // VLNEXT: if set, treat this char as literal (skip all special processing)
        if self.lnext {
            self.lnext = false;
            if canonical {
                self.line_buf.push(ch);
                if echo {
                    // Display as ^X for control chars, else the char itself
                    if ch < 0x20 {
                        self.output_buf.push_back(b'^');
                        self.output_buf.push_back(ch + 0x40);
                    } else {
                        self.output_buf.push_back(ch);
                    }
                }
            } else {
                self.input_buf.push_back(ch);
                if echo {
                    self.output_buf.push_back(ch);
                }
            }
            return;
        }

        // XON/XOFF flow control (P8.7)
        if ixon {
            if ch == self.termios.c_cc[VSTOP] {
                // Ctrl+S -> stop output
                self.output_stopped = true;
                serial_println!("[TTY{}] XOFF (Ctrl+S) - output stopped", self.id);
                return;
            }
            if ch == self.termios.c_cc[VSTART] {
                // Ctrl+Q -> resume output
                self.output_stopped = false;
                serial_println!("[TTY{}] XON (Ctrl+Q) - output resumed", self.id);
                return;
            }
            // Any input resumes output if stopped (IXANY behavior)
            if self.output_stopped {
                self.output_stopped = false;
            }
        }

        // Signal generation
        if isig {
            if ch == self.termios.c_cc[VINTR] {
                serial_println!("[TTY{}] SIGINT (Ctrl+C)", self.id);
                if self.fg_pgid > 0 {
                    let _ = crate::signals::kill(self.fg_pgid, crate::signals::Signal::SIGINT, 0);
                }
                return;
            }
            if ch == self.termios.c_cc[VQUIT] {
                serial_println!("[TTY{}] SIGQUIT (Ctrl+\\)", self.id);
                if self.fg_pgid > 0 {
                    let _ = crate::signals::kill(self.fg_pgid, crate::signals::Signal::SIGQUIT, 0);
                }
                return;
            }
            if ch == self.termios.c_cc[VSUSP] {
                serial_println!("[TTY{}] SIGTSTP (Ctrl+Z)", self.id);
                if self.fg_pgid > 0 {
                    let _ = crate::signals::kill(self.fg_pgid, crate::signals::Signal::SIGTSTP, 0);
                }
                return;
            }
        }

        // IEXTEN: Extended input processing (P8.8)
        if iexten && canonical {
            // VLNEXT (Ctrl+V) — make next character literal
            if ch == self.termios.c_cc[VLNEXT] {
                self.lnext = true;
                if echo {
                    // Show ^? that will be overwritten by the literal char
                    self.output_buf.push_back(b'^');
                    self.output_buf.push_back(8); // BS to position for overwrite
                }
                return;
            }
            // VWERASE (Ctrl+W) — word erase
            if ch == self.termios.c_cc[VWERASE] {
                self.word_erase(echo);
                return;
            }
            // VREPRINT (Ctrl+R) — reprint line
            if ch == self.termios.c_cc[VREPRINT] {
                if echo {
                    self.output_buf.push_back(b'^');
                    self.output_buf.push_back(b'R');
                    self.output_buf.push_back(b'\n');
                    for &b in &self.line_buf {
                        self.output_buf.push_back(b);
                    }
                }
                return;
            }
        }

        // CR -> NL translation
        let ch = if self.termios.c_iflag & ICRNL != 0 && ch == b'\r' {
            b'\n'
        } else {
            ch
        };

        if canonical {
            // Canonical mode: line-buffered editing
            match ch {
                // EOF (Ctrl+D)
                c if c == self.termios.c_cc[VEOF] => {
                    for &b in &self.line_buf {
                        self.input_buf.push_back(b);
                    }
                    self.line_buf.clear();
                }
                // Erase (Backspace/Delete)
                c if c == self.termios.c_cc[VERASE] => {
                    self.erase_char(echo);
                }
                // Kill (Ctrl+U) - erase entire line
                c if c == self.termios.c_cc[VKILL] => {
                    let len = self.line_buf.len();
                    self.line_buf.clear();
                    if echo {
                        for _ in 0..len {
                            self.output_buf.push_back(8);
                            self.output_buf.push_back(b' ');
                            self.output_buf.push_back(8);
                        }
                    }
                }
                // Newline - flush line
                b'\n' => {
                    self.line_buf.push(b'\n');
                    for &b in &self.line_buf {
                        self.input_buf.push_back(b);
                    }
                    self.line_buf.clear();
                    if echo {
                        self.output_buf.push_back(b'\n');
                    }
                }
                // Regular character
                _ => {
                    self.line_buf.push(ch);
                    if echo {
                        self.output_buf.push_back(ch);
                    }
                }
            }
        } else {
            // Raw mode: pass through directly
            self.input_buf.push_back(ch);
            if echo {
                self.output_buf.push_back(ch);
            }
        }
    }

    /// Erase a single character, UTF-8 aware when IUTF8 is set (P8.9)
    fn erase_char(&mut self, echo: bool) {
        if self.line_buf.is_empty() {
            return;
        }
        let iutf8 = self.termios.c_lflag & IUTF8 != 0;
        if iutf8 {
            // UTF-8 aware: erase the last complete codepoint
            let mut erase_count = 0;
            // Walk backwards over UTF-8 continuation bytes (10xxxxxx)
            while !self.line_buf.is_empty() {
                let last = self.line_buf[self.line_buf.len() - 1];
                self.line_buf.pop();
                erase_count += 1;
                // Stop after lead byte (0xxxxxxx or 11xxxxxx)
                if !(0x80..0xC0).contains(&last) {
                    break;
                }
            }
            if echo {
                // For multi-byte chars, they display as one glyph — erase one column
                // For ASCII, erase one column
                let cols = 1;
                for _ in 0..cols {
                    self.output_buf.push_back(8); // BS
                    self.output_buf.push_back(b' ');
                    self.output_buf.push_back(8); // BS
                }
            }
        } else {
            // Classic byte-at-a-time erase
            self.line_buf.pop();
            if echo {
                self.output_buf.push_back(8);
                self.output_buf.push_back(b' ');
                self.output_buf.push_back(8);
            }
        }
    }

    /// Word erase: erase backwards to the start of the previous word (P8.8)
    /// UTF-8 aware when IUTF8 is set
    fn word_erase(&mut self, echo: bool) {
        let iutf8 = self.termios.c_lflag & IUTF8 != 0;
        // Skip trailing whitespace
        while !self.line_buf.is_empty() {
            let last = *self.line_buf.last().unwrap();
            if last == b' ' || last == b'\t' {
                self.line_buf.pop();
                if echo {
                    self.output_buf.push_back(8);
                    self.output_buf.push_back(b' ');
                    self.output_buf.push_back(8);
                }
            } else {
                break;
            }
        }
        // Erase word characters
        while !self.line_buf.is_empty() {
            let last = *self.line_buf.last().unwrap();
            if last == b' ' || last == b'\t' {
                break;
            }
            if iutf8 {
                // Erase one UTF-8 codepoint
                let mut _bytes = 0;
                while !self.line_buf.is_empty() {
                    let b = self.line_buf[self.line_buf.len() - 1];
                    self.line_buf.pop();
                    _bytes += 1;
                    if !(0x80..0xC0).contains(&b) {
                        break;
                    }
                }
            } else {
                self.line_buf.pop();
            }
            if echo {
                self.output_buf.push_back(8);
                self.output_buf.push_back(b' ');
                self.output_buf.push_back(8);
            }
        }
    }

    /// Write output (from process)
    pub fn write_output(&mut self, data: &[u8]) {
        // XON/XOFF: if output is stopped, buffer but don't flush (P8.7)
        // We still accept into the buffer so data isn't lost
        let opost = self.termios.c_oflag & OPOST != 0;
        let onlcr = self.termios.c_oflag & ONLCR != 0;

        for &byte in data {
            if opost && onlcr && byte == b'\n' {
                self.output_buf.push_back(b'\r');
            }
            self.output_buf.push_back(byte);

            // Track column for tab stops
            match byte {
                b'\n' | b'\r' => self.column = 0,
                b'\t' => self.column = (self.column + 8) & !7,
                _ => self.column += 1,
            }
        }
    }

    /// Read available input
    pub fn read_input(&mut self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        while count < buf.len() {
            if let Some(byte) = self.input_buf.pop_front() {
                buf[count] = byte;
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Flush output buffer, returning bytes to render
    /// When XON/XOFF is active and output is stopped, returns empty (P8.7)
    pub fn flush_output(&mut self) -> Vec<u8> {
        if self.output_stopped {
            return Vec::new();
        }
        let output: Vec<u8> = self.output_buf.drain(..).collect();
        output
    }

    /// Set terminal attributes
    pub fn set_termios(&mut self, termios: Termios) {
        self.termios = termios;
    }

    /// Get terminal attributes
    pub fn get_termios(&self) -> Termios {
        self.termios
    }

    /// Set window size and deliver SIGWINCH to foreground process group
    pub fn set_winsize(&mut self, ws: WinSize) {
        let old = self.winsize;
        self.winsize = ws;
        // Send SIGWINCH to foreground process group if size actually changed
        if (old.ws_row != ws.ws_row || old.ws_col != ws.ws_col) && self.fg_pgid > 0 {
            let pgid = self.fg_pgid;
            // Deliver SIGWINCH (signal 28) to the foreground process group
            let _ = crate::pgrp::killpg(pgid, crate::signals::Signal::SIGWINCH);
            crate::serial_println!(
                "[TTY] SIGWINCH delivered to pgid {} ({}x{} -> {}x{})",
                pgid,
                old.ws_col,
                old.ws_row,
                ws.ws_col,
                ws.ws_row
            );
        }
    }

    /// Check if input is available
    pub fn input_available(&self) -> bool {
        !self.input_buf.is_empty()
    }
}

/// Global TTY table
lazy_static::lazy_static! {
    pub static ref TTY_TABLE: Mutex<Vec<Tty>> = {
        let mut ttys = Vec::new();
        // TTY0 - main console
        let mut tty0 = Tty::new(0, "tty0");
        tty0.is_open = true;
        tty0.fg_pgid = 2; // knoxos-desktop
        ttys.push(tty0);

        // TTY1-TTY6 - virtual consoles
        for i in 1..=6 {
            ttys.push(Tty::new(i, &alloc::format!("tty{}", i)));
        }

        // pts/0 - pseudo-terminal for terminal emulator
        let mut pts0 = Tty::new(100, "pts/0");
        pts0.is_open = false;
        ttys.push(pts0);

        Mutex::new(ttys)
    };
}

static NEXT_PTY_ID: AtomicU32 = AtomicU32::new(101);

/// Currently active virtual console (0-6), used for Ctrl+Alt+Fn VT switching (P8.1)
pub static ACTIVE_CONSOLE: AtomicU32 = AtomicU32::new(0);

/// Switch to a virtual console by index (0-6) (P8.1)
pub fn switch_console(vt: u32) {
    if vt > 6 {
        return;
    }
    let old = ACTIVE_CONSOLE.swap(vt, Ordering::SeqCst);
    if old == vt {
        return;
    }
    serial_println!("[KnoxOS] VT switch: tty{} -> tty{}", old, vt);

    let mut ttys = TTY_TABLE.lock();
    // Deactivate old console
    if let Some(old_tty) = ttys.iter_mut().find(|t| t.id == old) {
        old_tty.is_open = false;
    }
    // Activate new console
    if let Some(new_tty) = ttys.iter_mut().find(|t| t.id == vt) {
        new_tty.is_open = true;
    }
}

/// Get the currently active console ID
pub fn active_console() -> u32 {
    ACTIVE_CONSOLE.load(Ordering::Relaxed)
}

/// Allocate a new pseudo-terminal
pub fn alloc_pty() -> u32 {
    let id = NEXT_PTY_ID.fetch_add(1, Ordering::Relaxed);
    let pty = Tty::new(id, &alloc::format!("pts/{}", id - 100));
    TTY_TABLE.lock().push(pty);
    serial_println!("[KnoxOS] Allocated PTY pts/{}", id - 100);
    id
}

/// Get a TTY by ID
pub fn get_tty(id: u32) -> Option<usize> {
    TTY_TABLE.lock().iter().position(|t| t.id == id)
}

/// Feed a character to a TTY
pub fn tty_input(id: u32, ch: u8) {
    let mut ttys = TTY_TABLE.lock();
    if let Some(tty) = ttys.iter_mut().find(|t| t.id == id) {
        tty.input_char(ch);
    }
}

/// Write to a TTY
pub fn tty_write(id: u32, data: &[u8]) {
    let mut ttys = TTY_TABLE.lock();
    if let Some(tty) = ttys.iter_mut().find(|t| t.id == id) {
        tty.write_output(data);
    }
}

/// Read from a TTY
pub fn tty_read(id: u32, buf: &mut [u8]) -> usize {
    let mut ttys = TTY_TABLE.lock();
    if let Some(tty) = ttys.iter_mut().find(|t| t.id == id) {
        tty.read_input(buf)
    } else {
        0
    }
}

/// Initialize TTY subsystem
pub fn init() {
    // Force lazy init
    let ttys = TTY_TABLE.lock();
    serial_println!(
        "[KnoxOS] TTY subsystem initialized ({} terminals)",
        ttys.len()
    );
}
