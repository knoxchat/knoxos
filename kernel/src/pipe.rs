// pipe.rs — Full pipe infrastructure with proper Linux semantics
// Supports pipe(), pipe2(), O_NONBLOCK, O_CLOEXEC, PIPE_BUF atomicity

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// Pipe buffer size (matches Linux default)
const PIPE_BUF_SIZE: usize = 65536;

/// POSIX PIPE_BUF: writes of this size or less are atomic
const PIPE_BUF: usize = 4096;

/// Maximum pipe buffer pages (Linux default 16)
const PIPE_MAX_PAGES: usize = 16;

/// Pipe flags
pub const O_NONBLOCK: u32 = 0x800;
pub const O_CLOEXEC: u32 = 0x80000;
pub const O_DIRECT: u32 = 0x4000;

/// F_SETPIPE_SZ / F_GETPIPE_SZ
pub const F_SETPIPE_SZ: u32 = 1031;
pub const F_GETPIPE_SZ: u32 = 1032;

/// Pipe state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PipeState {
    Open,
    ReadClosed,
    WriteClosed,
    FullyClosed,
}

/// A single pipe instance
#[derive(Debug)]
pub struct Pipe {
    pub id: u64,
    pub buffer: Vec<u8>,
    pub read_pos: usize,
    pub write_pos: usize,
    pub data_len: usize,
    pub capacity: usize,
    pub flags: u32,
    pub readers: u32,
    pub writers: u32,
    pub state: PipeState,
    pub bytes_read: u64,
    pub bytes_written: u64,
    /// Processes waiting to read
    pub read_waiters: Vec<u64>,
    /// Processes waiting to write
    pub write_waiters: Vec<u64>,
}

impl Pipe {
    pub fn new(id: u64, flags: u32) -> Self {
        Pipe {
            id,
            buffer: alloc::vec![0u8; PIPE_BUF_SIZE],
            read_pos: 0,
            write_pos: 0,
            data_len: 0,
            capacity: PIPE_BUF_SIZE,
            flags,
            readers: 1,
            writers: 1,
            state: PipeState::Open,
            bytes_read: 0,
            bytes_written: 0,
            read_waiters: Vec::new(),
            write_waiters: Vec::new(),
        }
    }

    /// Write data to the pipe
    pub fn write(&mut self, data: &[u8]) -> Result<usize, PipeError> {
        if self.readers == 0 {
            return Err(PipeError::BrokenPipe); // EPIPE / SIGPIPE
        }

        if data.is_empty() {
            return Ok(0);
        }

        let available = self.capacity - self.data_len;
        if available == 0 {
            if self.flags & O_NONBLOCK != 0 {
                return Err(PipeError::WouldBlock);
            }
            return Err(PipeError::WouldBlock); // caller should retry
        }

        // Atomic write guarantee for <= PIPE_BUF
        let to_write = if data.len() <= PIPE_BUF {
            if available < data.len() {
                if self.flags & O_NONBLOCK != 0 {
                    return Err(PipeError::WouldBlock);
                }
                return Err(PipeError::WouldBlock);
            }
            data.len()
        } else {
            core::cmp::min(data.len(), available)
        };

        // Copy data into circular buffer
        for item in data.iter().take(to_write) {
            self.buffer[self.write_pos] = *item;
            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
        self.data_len += to_write;
        self.bytes_written += to_write as u64;

        // Wake up readers
        self.read_waiters.clear();

        Ok(to_write)
    }

    /// Read data from the pipe
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, PipeError> {
        if self.data_len == 0 {
            if self.writers == 0 {
                return Ok(0); // EOF
            }
            if self.flags & O_NONBLOCK != 0 {
                return Err(PipeError::WouldBlock);
            }
            return Err(PipeError::WouldBlock);
        }

        let to_read = core::cmp::min(buf.len(), self.data_len);

        for item in buf.iter_mut().take(to_read) {
            *item = self.buffer[self.read_pos];
            self.read_pos = (self.read_pos + 1) % self.capacity;
        }
        self.data_len -= to_read;
        self.bytes_read += to_read as u64;

        // Wake up writers
        self.write_waiters.clear();

        Ok(to_read)
    }

    /// Peek at data without consuming
    pub fn peek(&self, buf: &mut [u8]) -> usize {
        let to_peek = core::cmp::min(buf.len(), self.data_len);
        let mut pos = self.read_pos;
        for item in buf.iter_mut().take(to_peek) {
            *item = self.buffer[pos];
            pos = (pos + 1) % self.capacity;
        }
        to_peek
    }

    /// Set pipe buffer size (F_SETPIPE_SZ)
    pub fn set_size(&mut self, new_size: usize) -> Result<usize, PipeError> {
        let new_size = core::cmp::max(new_size, 4096); // minimum 1 page
        let max_size = PIPE_MAX_PAGES * 4096;
        let new_size = core::cmp::min(new_size, max_size);

        // Round up to power of 2
        let new_size = new_size.next_power_of_two();

        if new_size < self.data_len {
            return Err(PipeError::TooSmall);
        }

        // Reallocate buffer
        let mut new_buffer = alloc::vec![0u8; new_size];
        let mut pos = self.read_pos;
        for item in new_buffer.iter_mut().take(self.data_len) {
            *item = self.buffer[pos];
            pos = (pos + 1) % self.capacity;
        }

        self.buffer = new_buffer;
        self.capacity = new_size;
        self.read_pos = 0;
        self.write_pos = self.data_len;

        Ok(new_size)
    }

    /// Get available space for writing
    pub fn write_space(&self) -> usize {
        self.capacity - self.data_len
    }

    /// Get available data for reading
    pub fn read_available(&self) -> usize {
        self.data_len
    }

    /// Close the read end
    pub fn close_read(&mut self) {
        if self.readers > 0 {
            self.readers -= 1;
        }
        if self.readers == 0 {
            self.state = if self.writers == 0 {
                PipeState::FullyClosed
            } else {
                PipeState::ReadClosed
            };
        }
    }

    /// Close the write end
    pub fn close_write(&mut self) {
        if self.writers > 0 {
            self.writers -= 1;
        }
        if self.writers == 0 {
            self.state = if self.readers == 0 {
                PipeState::FullyClosed
            } else {
                PipeState::WriteClosed
            };
        }
    }

    /// Check if pipe is broken (no readers for write, no writers for read)
    pub fn is_broken(&self) -> bool {
        matches!(self.state, PipeState::ReadClosed | PipeState::FullyClosed)
    }
}

/// Pipe errors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PipeError {
    BrokenPipe, // EPIPE
    WouldBlock, // EAGAIN
    InvalidFd,  // EBADF
    TooSmall,   // EBUSY
    NotFound,   // ENOENT
}

lazy_static! {
    static ref PIPES: Mutex<PipeTable> = Mutex::new(PipeTable::new());
}

struct PipeTable {
    pipes: BTreeMap<u64, Pipe>,
    next_id: u64,
}

impl PipeTable {
    fn new() -> Self {
        PipeTable {
            pipes: BTreeMap::new(),
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Create a pipe, returns (read_fd_id, write_fd_id)
pub fn sys_pipe(flags: u32) -> Result<(u64, u64), PipeError> {
    let mut table = PIPES.lock();
    let pipe_id = table.alloc_id();
    let pipe = Pipe::new(pipe_id, flags);
    table.pipes.insert(pipe_id, pipe);
    // Return pipe_id for both ends — caller maps to process fd table
    // Read end = pipe_id * 2, Write end = pipe_id * 2 + 1
    Ok((pipe_id * 2, pipe_id * 2 + 1))
}

/// Write to a pipe by ID
pub fn pipe_write(pipe_id: u64, data: &[u8]) -> Result<usize, PipeError> {
    let mut table = PIPES.lock();
    let pipe = table.pipes.get_mut(&pipe_id).ok_or(PipeError::NotFound)?;
    pipe.write(data)
}

/// Read from a pipe by ID
pub fn pipe_read(pipe_id: u64, buf: &mut [u8]) -> Result<usize, PipeError> {
    let mut table = PIPES.lock();
    let pipe = table.pipes.get_mut(&pipe_id).ok_or(PipeError::NotFound)?;
    pipe.read(buf)
}

/// Close a pipe end
pub fn pipe_close(pipe_id: u64, is_read_end: bool) {
    let mut table = PIPES.lock();
    if let Some(pipe) = table.pipes.get_mut(&pipe_id) {
        if is_read_end {
            pipe.close_read();
        } else {
            pipe.close_write();
        }
        if pipe.state == PipeState::FullyClosed {
            table.pipes.remove(&pipe_id);
        }
    }
}

/// Get pipe buffer size (F_GETPIPE_SZ)
pub fn pipe_get_size(pipe_id: u64) -> Result<usize, PipeError> {
    let table = PIPES.lock();
    let pipe = table.pipes.get(&pipe_id).ok_or(PipeError::NotFound)?;
    Ok(pipe.capacity)
}

/// Set pipe buffer size (F_SETPIPE_SZ)
pub fn pipe_set_size(pipe_id: u64, size: usize) -> Result<usize, PipeError> {
    let mut table = PIPES.lock();
    let pipe = table.pipes.get_mut(&pipe_id).ok_or(PipeError::NotFound)?;
    pipe.set_size(size)
}

/// Splice data between two pipes (zero-copy)
pub fn pipe_splice(src_id: u64, dst_id: u64, len: usize) -> Result<usize, PipeError> {
    let mut table = PIPES.lock();

    // Read from source
    let mut tmp = alloc::vec![0u8; len];
    let n = {
        let src = table.pipes.get_mut(&src_id).ok_or(PipeError::NotFound)?;
        src.read(&mut tmp)?
    };

    // Write to destination
    let written = {
        let dst = table.pipes.get_mut(&dst_id).ok_or(PipeError::NotFound)?;
        dst.write(&tmp[..n])?
    };

    Ok(written)
}

/// Tee — duplicate pipe data without consuming
pub fn pipe_tee(src_id: u64, dst_id: u64, len: usize) -> Result<usize, PipeError> {
    let mut table = PIPES.lock();

    // Peek from source (don't consume)
    let mut tmp = alloc::vec![0u8; len];
    let n = {
        let src = table.pipes.get(&src_id).ok_or(PipeError::NotFound)?;
        src.peek(&mut tmp)
    };

    // Write to destination
    let written = {
        let dst = table.pipes.get_mut(&dst_id).ok_or(PipeError::NotFound)?;
        dst.write(&tmp[..n])?
    };

    Ok(written)
}

/// Get pipe info for /proc/[pid]/fdinfo
pub fn pipe_info(pipe_id: u64) -> Option<PipeInfo> {
    let table = PIPES.lock();
    table.pipes.get(&pipe_id).map(|p| PipeInfo {
        id: p.id,
        capacity: p.capacity,
        data_len: p.data_len,
        readers: p.readers,
        writers: p.writers,
        bytes_read: p.bytes_read,
        bytes_written: p.bytes_written,
        flags: p.flags,
    })
}

#[derive(Debug, Clone)]
pub struct PipeInfo {
    pub id: u64,
    pub capacity: usize,
    pub data_len: usize,
    pub readers: u32,
    pub writers: u32,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub flags: u32,
}

/// Create a new pipe, returning (read_fd, write_fd)
pub fn create_pipe() -> (i32, i32) {
    match sys_pipe(0) {
        Ok((read_id, write_id)) => (read_id as i32, write_id as i32),
        Err(_) => (-1, -1),
    }
}

/// Initialize pipe subsystem
pub fn init() {
    crate::serial_println!(
        "  Pipe subsystem initialized (PIPE_BUF={}, max={}KB)",
        PIPE_BUF,
        PIPE_BUF_SIZE / 1024
    );
}
