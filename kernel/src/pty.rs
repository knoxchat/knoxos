/// Pseudo-Terminal (PTY) — Linux-compatible PTY master/slave pairs
/// Implements the Unix98 (devpts) PTY interface
///
/// PTYs provide a bidirectional communication channel that appears
/// as a terminal on the slave side. Used by terminal emulators, SSH,
/// screen/tmux, expect, etc.
///
/// API:
///   - openpty() / posix_openpt() → (master_fd, slave_fd)
///   - grantpt() / unlockpt()
///   - ptsname() → "/dev/pts/N"
///   - Read/write on master ↔ slave
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Maximum number of PTY pairs
const MAX_PTYS: usize = 256;

/// PTY buffer size
const PTY_BUFFER_SIZE: usize = 4096;

/// A PTY pair (master + slave)
#[derive(Debug)]
pub struct PtyPair {
    /// PTY index number (for /dev/pts/N)
    pub index: u32,
    /// Master → slave buffer (data written to master appears on slave)
    master_to_slave: Vec<u8>,
    /// Slave → master buffer (data written to slave appears on master)
    slave_to_master: Vec<u8>,
    /// Terminal size
    pub winsize: WinSize,
    /// Whether the slave is open
    pub slave_open: bool,
    /// Whether the master is open
    pub master_open: bool,
    /// Terminal attributes (simplified termios)
    pub termios: Termios,
    /// Process group controlling the terminal
    pub foreground_pgid: u32,
    /// Session ID
    pub session_id: u32,
    /// Whether the slave has been granted (grantpt)
    pub granted: bool,
    /// Whether the slave has been unlocked (unlockpt)
    pub unlocked: bool,
    /// Packet mode enabled (TIOCPKT)
    pub packet_mode: bool,
    /// Packet mode status byte
    pub packet_status: u8,
}

/// Terminal window size
#[derive(Debug, Clone, Copy)]
#[repr(C)]
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
            ws_xpixel: 640,
            ws_ypixel: 400,
        }
    }
}

/// Simplified termios structure
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Termios {
    pub c_iflag: u32,   // Input flags
    pub c_oflag: u32,   // Output flags
    pub c_cflag: u32,   // Control flags
    pub c_lflag: u32,   // Local flags
    pub c_cc: [u8; 20], // Control characters
}

impl Default for Termios {
    fn default() -> Self {
        let mut cc = [0u8; 20];
        cc[0] = 0x03; // VINTR = Ctrl-C
        cc[1] = 0x1C; // VQUIT = Ctrl-\
        cc[2] = 0x08; // VERASE = Backspace
        cc[3] = 0x15; // VKILL = Ctrl-U
        cc[4] = 0x04; // VEOF = Ctrl-D
        cc[5] = 0; // VTIME
        cc[6] = 1; // VMIN
        cc[8] = 0x11; // VSTART = Ctrl-Q
        cc[9] = 0x13; // VSTOP = Ctrl-S
        cc[10] = 0x1A; // VSUSP = Ctrl-Z

        Self {
            c_iflag: 0o0002 | 0o0400,                   // ICRNL | IXON
            c_oflag: 0o0001 | 0o0004,                   // OPOST | ONLCR
            c_cflag: 0o0060 | 0o0200 | 0o0015,          // CS8 | CREAD | B38400
            c_lflag: 0o0001 | 0o0002 | 0o0010 | 0o0100, // ISIG | ICANON | ECHO | ECHOE
            c_cc: cc,
        }
    }
}

/// Termios local flags
pub const ISIG: u32 = 0o0001;
pub const ICANON: u32 = 0o0002;
pub const ECHO: u32 = 0o0010;
pub const ECHOE: u32 = 0o0100;
pub const ECHOK: u32 = 0o0200;
pub const ECHOCTL: u32 = 0o1000;

impl PtyPair {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            master_to_slave: Vec::with_capacity(PTY_BUFFER_SIZE),
            slave_to_master: Vec::with_capacity(PTY_BUFFER_SIZE),
            winsize: WinSize::default(),
            slave_open: false,
            master_open: true,
            termios: Termios::default(),
            foreground_pgid: 0,
            session_id: 0,
            granted: false,
            unlocked: false,
            packet_mode: false,
            packet_status: 0,
        }
    }

    /// Write to master (data goes to slave's input, with ISIG processing)
    pub fn write_master(&mut self, data: &[u8]) -> Result<usize, i32> {
        if !self.slave_open {
            return Err(-5); // EIO
        }

        // ISIG: check for signal-generating characters
        if self.termios.c_lflag & ISIG != 0 {
            for &byte in data {
                if byte == self.termios.c_cc[0] {
                    // VINTR (Ctrl+C) → SIGINT
                    self.send_signal_to_foreground(crate::signals::Signal::SIGINT);
                    return Ok(data.len());
                } else if byte == self.termios.c_cc[1] {
                    // VQUIT (Ctrl+\) → SIGQUIT
                    self.send_signal_to_foreground(crate::signals::Signal::SIGQUIT);
                    return Ok(data.len());
                } else if byte == self.termios.c_cc[10] {
                    // VSUSP (Ctrl+Z) → SIGTSTP
                    self.send_signal_to_foreground(crate::signals::Signal::SIGTSTP);
                    // Also update the shell job table
                    crate::shell::env::stop_foreground_job();
                    return Ok(data.len());
                }
            }
        }

        let space = PTY_BUFFER_SIZE - self.master_to_slave.len();
        let to_write = data.len().min(space);
        self.master_to_slave.extend_from_slice(&data[..to_write]);
        Ok(to_write)
    }

    /// Send a signal to the foreground process group of this PTY
    fn send_signal_to_foreground(&self, sig: crate::signals::Signal) {
        if self.foreground_pgid != 0 {
            let _ = crate::pgrp::killpg(self.foreground_pgid, sig);
        }
    }

    /// Read from master (gets data from slave's output)
    /// In packet mode, prepends a status byte to each read
    pub fn read_master(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        if self.packet_mode {
            // Packet mode: first byte is status, rest is data
            if self.packet_status != 0 {
                // Report status change
                if buf.is_empty() {
                    return Err(-11); // EAGAIN
                }
                buf[0] = self.packet_status;
                self.packet_status = 0;
                return Ok(1);
            }
            if self.slave_to_master.is_empty() {
                if !self.slave_open {
                    return Ok(0); // EOF
                }
                return Err(-11); // EAGAIN
            }
            if buf.len() < 2 {
                return Err(-11); // EAGAIN - need room for status + data
            }
            buf[0] = TIOCPKT_DATA; // Normal data
            let to_read = (buf.len() - 1).min(self.slave_to_master.len());
            buf[1..1 + to_read].copy_from_slice(&self.slave_to_master[..to_read]);
            self.slave_to_master.drain(..to_read);
            return Ok(to_read + 1);
        }

        // Normal mode
        if self.slave_to_master.is_empty() {
            if !self.slave_open {
                return Ok(0); // EOF
            }
            return Err(-11); // EAGAIN
        }
        let to_read = buf.len().min(self.slave_to_master.len());
        buf[..to_read].copy_from_slice(&self.slave_to_master[..to_read]);
        self.slave_to_master.drain(..to_read);
        Ok(to_read)
    }

    /// Write to slave (data goes to master's input, with echo processing)
    pub fn write_slave(&mut self, data: &[u8]) -> Result<usize, i32> {
        if !self.master_open {
            return Err(-5); // EIO
        }

        let mut processed = Vec::new();
        for &byte in data {
            // Output processing (OPOST)
            if self.termios.c_oflag & 0o0001 != 0 {
                // ONLCR: map NL to CR-NL
                if byte == b'\n' && self.termios.c_oflag & 0o0004 != 0 {
                    processed.push(b'\r');
                    processed.push(b'\n');
                    continue;
                }
            }
            processed.push(byte);
        }

        let space = PTY_BUFFER_SIZE - self.slave_to_master.len();
        let to_write = processed.len().min(space);
        self.slave_to_master
            .extend_from_slice(&processed[..to_write]);
        Ok(data.len().min(to_write))
    }

    /// Read from slave (gets input from master, with line discipline)
    pub fn read_slave(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        if self.master_to_slave.is_empty() {
            if !self.master_open {
                return Ok(0); // EOF
            }
            return Err(-11); // EAGAIN
        }

        // Canonical mode: return lines
        if self.termios.c_lflag & ICANON != 0 {
            if let Some(newline_pos) = self.master_to_slave.iter().position(|&b| b == b'\n') {
                let to_read = (newline_pos + 1).min(buf.len());
                buf[..to_read].copy_from_slice(&self.master_to_slave[..to_read]);
                self.master_to_slave.drain(..to_read);
                return Ok(to_read);
            }
            // No complete line yet
            return Err(-11); // EAGAIN
        }

        // Raw mode: return immediately
        let to_read = buf.len().min(self.master_to_slave.len());
        buf[..to_read].copy_from_slice(&self.master_to_slave[..to_read]);
        self.master_to_slave.drain(..to_read);
        Ok(to_read)
    }

    /// Get the slave device name
    pub fn pts_name(&self) -> String {
        alloc::format!("/dev/pts/{}", self.index)
    }

    /// Resize the terminal
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.winsize.ws_row = rows;
        self.winsize.ws_col = cols;
    }
}

lazy_static::lazy_static! {
    /// Global PTY table
    static ref PTY_TABLE: Mutex<BTreeMap<u32, PtyPair>> = Mutex::new(BTreeMap::new());
    /// Next PTY index
    static ref NEXT_PTY_INDEX: Mutex<u32> = Mutex::new(0);
}

/// Allocate a new PTY pair
/// Returns (master_fd_index, slave_path)
/// The slave starts locked; call grantpt()/unlockpt() or use posix_openpt() flow.
/// For convenience, openpty() auto-grants and unlocks (BSD behavior).
pub fn openpty() -> Result<(u32, String), i32> {
    let mut next = NEXT_PTY_INDEX.lock();
    if *next >= MAX_PTYS as u32 {
        return Err(-23); // ENFILE
    }

    let index = *next;
    *next += 1;
    drop(next);

    let mut pty = PtyPair::new(index);
    // openpty() automatically grants and unlocks (BSD convenience)
    pty.granted = true;
    pty.unlocked = true;
    let path = pty.pts_name();

    PTY_TABLE.lock().insert(index, pty);

    serial_println!("[pty] openpty: allocated pts/{}", index);
    Ok((index, path))
}

/// posix_openpt() — POSIX-compliant PTY allocation
/// Returns the master index. Caller must call grantpt() and unlockpt() before opening slave.
pub fn posix_openpt() -> Result<u32, i32> {
    let mut next = NEXT_PTY_INDEX.lock();
    if *next >= MAX_PTYS as u32 {
        return Err(-23); // ENFILE
    }

    let index = *next;
    *next += 1;
    drop(next);

    let pty = PtyPair::new(index);
    // NOT auto-granted/unlocked — caller must do grantpt() + unlockpt()

    PTY_TABLE.lock().insert(index, pty);

    serial_println!("[pty] posix_openpt: allocated master for pts/{}", index);
    Ok(index)
}

/// Open the slave side of a PTY
/// Requires that unlockpt() has been called first
pub fn open_slave(index: u32) -> Result<(), i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-2i32)?;
    if !pty.unlocked {
        return Err(-13); // EACCES — slave not unlocked
    }
    pty.slave_open = true;
    Ok(())
}

/// Close the master side of a PTY
pub fn close_master(index: u32) -> Result<(), i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-2i32)?;
    pty.master_open = false;
    // If slave is also closed, remove the PTY
    if !pty.slave_open {
        table.remove(&index);
    }
    Ok(())
}

/// Close the slave side of a PTY
pub fn close_slave(index: u32) -> Result<(), i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-2i32)?;
    pty.slave_open = false;
    // If master is also closed, remove the PTY
    if !pty.master_open {
        table.remove(&index);
    }
    Ok(())
}

/// Write to master side
pub fn write_master(index: u32, data: &[u8]) -> Result<usize, i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?;
    pty.write_master(data)
}

/// Read from master side
pub fn read_master(index: u32, buf: &mut [u8]) -> Result<usize, i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?;
    pty.read_master(buf)
}

/// Write to slave side
pub fn write_slave(index: u32, data: &[u8]) -> Result<usize, i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?;
    pty.write_slave(data)
}

/// Read from slave side
pub fn read_slave(index: u32, buf: &mut [u8]) -> Result<usize, i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?;
    pty.read_slave(buf)
}

/// Get terminal window size
pub fn get_winsize(index: u32) -> Result<WinSize, i32> {
    let table = PTY_TABLE.lock();
    let pty = table.get(&index).ok_or(-9i32)?;
    Ok(pty.winsize)
}

/// Set terminal window size
pub fn set_winsize(index: u32, ws: WinSize) -> Result<(), i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?;
    pty.winsize = ws;
    Ok(())
}

/// Get terminal attributes (termios)
pub fn get_termios(index: u32) -> Result<Termios, i32> {
    let table = PTY_TABLE.lock();
    let pty = table.get(&index).ok_or(-9i32)?;
    Ok(pty.termios)
}

/// Set terminal attributes (termios)
pub fn set_termios(index: u32, termios: Termios) -> Result<(), i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?;
    pty.termios = termios;
    Ok(())
}

/// List all active PTY pairs
pub fn list_ptys() -> Vec<u32> {
    PTY_TABLE.lock().keys().cloned().collect()
}

/// Get PTY count
pub fn count() -> usize {
    PTY_TABLE.lock().len()
}

// ── Packet mode constants ──────────────────────────────────────

/// Normal data (no control event)
pub const TIOCPKT_DATA: u8 = 0;
/// Ctrl+S (STOP) was sent
pub const TIOCPKT_STOP: u8 = 1 << 0;
/// Ctrl+Q (START) was sent
pub const TIOCPKT_START: u8 = 1 << 1;
/// Output was flushed
pub const TIOCPKT_FLUSHWRITE: u8 = 1 << 2;
/// Input was flushed
pub const TIOCPKT_FLUSHREAD: u8 = 1 << 3;
/// termios changed on the slave
pub const TIOCPKT_IOCTL: u8 = 1 << 4;
/// Stop/start from NOSTOP mode
pub const TIOCPKT_NOSTOP: u8 = 1 << 5;
pub const TIOCPKT_DOSTOP: u8 = 1 << 6;

// ── ioctl constants ────────────────────────────────────────────

/// Get PTY number (TIOCGPTN)
pub const TIOCGPTN: u32 = 0x80045430;
/// Lock/unlock PTY (TIOCSPTLCK)
pub const TIOCSPTLCK: u32 = 0x40045431;
/// Set packet mode (TIOCPKT)
pub const TIOCPKT: u32 = 0x5420;
/// Get foreground pgid (TIOCGPGRP)
pub const TIOCGPGRP: u32 = 0x540F;
/// Set foreground pgid (TIOCSPGRP)
pub const TIOCSPGRP: u32 = 0x5410;
/// Get winsize (TIOCGWINSZ)
pub const TIOCGWINSZ: u32 = 0x5413;
/// Set winsize (TIOCSWINSZ)
pub const TIOCSWINSZ: u32 = 0x5414;

/// grantpt() — set ownership and permissions on the slave PTY
/// In a real OS this sets the slave device to be owned by the calling user
/// with mode 0620 (user rw, group tty write). In our simplified model,
/// we just mark it as granted.
pub fn grantpt(index: u32) -> Result<(), i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?; // EBADF
    pty.granted = true;
    serial_println!("[pty] grantpt: pts/{} granted", index);
    Ok(())
}

/// unlockpt() — unlock the slave PTY so it can be opened
/// The slave side cannot be opened until unlockpt() is called.
pub fn unlockpt(index: u32) -> Result<(), i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?; // EBADF
    if !pty.granted {
        return Err(-1); // Must call grantpt first
    }
    pty.unlocked = true;
    serial_println!("[pty] unlockpt: pts/{} unlocked", index);
    Ok(())
}

/// ptsname() — get the name of the slave PTY
pub fn ptsname(index: u32) -> Result<String, i32> {
    let table = PTY_TABLE.lock();
    let pty = table.get(&index).ok_or(-9i32)?;
    Ok(pty.pts_name())
}

/// ioctl() — handle PTY ioctl requests
pub fn pty_ioctl(index: u32, request: u32, arg: u64) -> Result<u64, i32> {
    let mut table = PTY_TABLE.lock();
    let pty = table.get_mut(&index).ok_or(-9i32)?;

    match request {
        TIOCGPTN => {
            // Get PTY number — return the index
            Ok(pty.index as u64)
        }
        TIOCSPTLCK => {
            // Lock/unlock: arg = 0 to unlock, nonzero to lock
            pty.unlocked = arg == 0;
            Ok(0)
        }
        TIOCPKT => {
            // Enable/disable packet mode: arg = 0 to disable, nonzero to enable
            pty.packet_mode = arg != 0;
            serial_println!(
                "[pty] pts/{} packet mode: {}",
                index,
                if pty.packet_mode {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            Ok(0)
        }
        TIOCGPGRP => {
            // Get foreground process group
            Ok(pty.foreground_pgid as u64)
        }
        TIOCSPGRP => {
            // Set foreground process group
            pty.foreground_pgid = arg as u32;
            Ok(0)
        }
        TIOCGWINSZ => {
            // Return packed winsize: rows in upper 16 bits, cols in lower 16
            Ok(((pty.winsize.ws_row as u64) << 16) | (pty.winsize.ws_col as u64))
        }
        TIOCSWINSZ => {
            // Set winsize from packed value
            let old_rows = pty.winsize.ws_row;
            let old_cols = pty.winsize.ws_col;
            pty.winsize.ws_row = ((arg >> 16) & 0xFFFF) as u16;
            pty.winsize.ws_col = (arg & 0xFFFF) as u16;
            // Send SIGWINCH if size changed
            if (pty.winsize.ws_row != old_rows || pty.winsize.ws_col != old_cols)
                && pty.foreground_pgid != 0
            {
                let pgid = pty.foreground_pgid;
                drop(table);
                let _ = crate::pgrp::killpg(pgid, crate::signals::Signal::SIGWINCH);
            }
            Ok(0)
        }
        _ => {
            serial_println!("[pty] pts/{} unknown ioctl: {:#x}", index, request);
            Err(-25) // ENOTTY
        }
    }
}

/// Initialize PTY subsystem
pub fn init() {
    // Create /dev/pts directory in VFS
    let mut vfs = crate::vfs::VFS.lock();
    let _ = vfs.mkdir("/dev/pts", 0o755);
    drop(vfs);

    // Create /dev/ptmx device
    let mut vfs = crate::vfs::VFS.lock();
    vfs.create_file_at_path("/dev/ptmx", crate::vfs::FileType::CharDevice, &[], 0o666);
    drop(vfs);

    serial_println!("[KnoxOS] PTY subsystem initialized (Unix98 devpts)");
}
