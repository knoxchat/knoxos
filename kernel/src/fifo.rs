/// FIFO (Named Pipes) — Linux-compatible named pipe support
/// Provides mkfifo(2) semantics integrated with the VFS layer
///
/// FIFOs are special files that can be created in the filesystem
/// and used for IPC between unrelated processes. They behave like
/// anonymous pipes but are accessible via filesystem paths.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Size of FIFO buffer
const FIFO_BUFFER_SIZE: usize = 65536; // 64KB, same as Linux pipe

/// A FIFO (named pipe) buffer
#[derive(Debug)]
pub struct Fifo {
    /// The VFS path this FIFO is bound to
    pub path: String,
    /// Circular buffer for data
    buffer: Vec<u8>,
    /// Read position
    read_pos: usize,
    /// Write position
    write_pos: usize,
    /// Number of bytes currently in buffer
    count: usize,
    /// Number of open readers
    readers: u32,
    /// Number of open writers
    writers: u32,
    /// Whether the FIFO is in non-blocking mode
    pub nonblock: bool,
}

impl Fifo {
    pub fn new(path: &str) -> Self {
        Self {
            path: String::from(path),
            buffer: alloc::vec![0u8; FIFO_BUFFER_SIZE],
            read_pos: 0,
            write_pos: 0,
            count: 0,
            readers: 0,
            writers: 0,
            nonblock: false,
        }
    }

    /// Write data into the FIFO
    pub fn write(&mut self, data: &[u8]) -> Result<usize, i32> {
        if self.readers == 0 {
            return Err(-32); // EPIPE — no readers
        }

        let available = FIFO_BUFFER_SIZE - self.count;
        if available == 0 {
            if self.nonblock {
                return Err(-11); // EAGAIN
            }
            return Ok(0); // Would block
        }

        let to_write = data.len().min(available);
        for item in data.iter().take(to_write) {
            self.buffer[self.write_pos] = *item;
            self.write_pos = (self.write_pos + 1) % FIFO_BUFFER_SIZE;
        }
        self.count += to_write;
        Ok(to_write)
    }

    /// Read data from the FIFO
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        if self.count == 0 {
            if self.writers == 0 {
                return Ok(0); // EOF — no writers
            }
            if self.nonblock {
                return Err(-11); // EAGAIN
            }
            return Ok(0); // Would block
        }

        let to_read = buf.len().min(self.count);
        for item in buf.iter_mut().take(to_read) {
            *item = self.buffer[self.read_pos];
            self.read_pos = (self.read_pos + 1) % FIFO_BUFFER_SIZE;
        }
        self.count -= to_read;
        Ok(to_read)
    }

    /// Open the FIFO for reading
    pub fn open_read(&mut self) {
        self.readers += 1;
    }

    /// Open the FIFO for writing
    pub fn open_write(&mut self) {
        self.writers += 1;
    }

    /// Close a read end
    pub fn close_read(&mut self) {
        if self.readers > 0 {
            self.readers -= 1;
        }
    }

    /// Close a write end
    pub fn close_write(&mut self) {
        if self.writers > 0 {
            self.writers -= 1;
        }
    }

    /// Check if readable (has data or no writers)
    pub fn is_readable(&self) -> bool {
        self.count > 0 || self.writers == 0
    }

    /// Check if writable (has space and has readers)
    pub fn is_writable(&self) -> bool {
        self.count < FIFO_BUFFER_SIZE && self.readers > 0
    }

    /// Get number of bytes available to read
    pub fn available(&self) -> usize {
        self.count
    }
}

lazy_static::lazy_static! {
    /// Global FIFO table, keyed by path
    static ref FIFOS: Mutex<BTreeMap<String, Fifo>> = Mutex::new(BTreeMap::new());
    /// Next FIFO ID for internal tracking
    static ref NEXT_FIFO_ID: Mutex<u32> = Mutex::new(1);
}

/// Create a named pipe (mkfifo)
pub fn mkfifo(path: &str, _mode: u16) -> Result<(), i32> {
    let mut fifos = FIFOS.lock();
    if fifos.contains_key(path) {
        return Err(-17); // EEXIST
    }

    // Create the FIFO
    fifos.insert(String::from(path), Fifo::new(path));

    // Also create in VFS as a Pipe type file
    let mut vfs = crate::vfs::VFS.lock();
    if vfs.resolve_path(path).is_some() {
        return Err(-17); // EEXIST in VFS
    }
    vfs.create_file_at_path(path, crate::vfs::FileType::Pipe, &[], _mode);

    serial_println!("[fifo] mkfifo: {}", path);
    Ok(())
}

/// Open a FIFO for reading or writing
pub fn open_fifo(path: &str, writing: bool) -> Result<(), i32> {
    let mut fifos = FIFOS.lock();
    let fifo = fifos.get_mut(path).ok_or(-2i32)?; // ENOENT
    if writing {
        fifo.open_write();
    } else {
        fifo.open_read();
    }
    Ok(())
}

/// Write to a FIFO
pub fn write_fifo(path: &str, data: &[u8]) -> Result<usize, i32> {
    let mut fifos = FIFOS.lock();
    let fifo = fifos.get_mut(path).ok_or(-2i32)?;
    fifo.write(data)
}

/// Read from a FIFO
pub fn read_fifo(path: &str, buf: &mut [u8]) -> Result<usize, i32> {
    let mut fifos = FIFOS.lock();
    let fifo = fifos.get_mut(path).ok_or(-2i32)?;
    fifo.read(buf)
}

/// Close a FIFO end
pub fn close_fifo(path: &str, writing: bool) -> Result<(), i32> {
    let mut fifos = FIFOS.lock();
    let fifo = fifos.get_mut(path).ok_or(-2i32)?;
    if writing {
        fifo.close_write();
    } else {
        fifo.close_read();
    }

    // Remove FIFO if no readers and no writers
    if fifo.readers == 0 && fifo.writers == 0 {
        fifos.remove(path);
    }
    Ok(())
}

/// Remove a FIFO from the filesystem
pub fn unlink_fifo(path: &str) -> Result<(), i32> {
    FIFOS.lock().remove(path).ok_or(-2i32)?;
    Ok(())
}

/// Check if a path is a FIFO
pub fn is_fifo(path: &str) -> bool {
    FIFOS.lock().contains_key(path)
}

/// List all FIFOs
pub fn list_fifos() -> Vec<String> {
    FIFOS.lock().keys().cloned().collect()
}

/// Initialize FIFO subsystem
pub fn init() {
    serial_println!("[KnoxOS] FIFO (named pipes) subsystem initialized");
}
