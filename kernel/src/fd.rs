/// File Descriptor Table - Per-process file descriptor management
/// Implements Linux-compatible file descriptor semantics
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// File descriptor number
pub type Fd = i32;

/// Standard file descriptors
pub const STDIN_FD: Fd = 0;
pub const STDOUT_FD: Fd = 1;
pub const STDERR_FD: Fd = 2;

/// Maximum file descriptors per process
pub const MAX_FDS: usize = 256;

/// File open flags (Linux-compatible)
#[derive(Debug, Clone, Copy)]
pub struct OpenFlags(pub u32);

impl OpenFlags {
    pub const O_RDONLY: u32 = 0x0000;
    pub const O_WRONLY: u32 = 0x0001;
    pub const O_RDWR: u32 = 0x0002;
    pub const O_CREAT: u32 = 0x0040;
    pub const O_EXCL: u32 = 0x0080;
    pub const O_TRUNC: u32 = 0x0200;
    pub const O_APPEND: u32 = 0x0400;
    pub const O_NONBLOCK: u32 = 0x0800;
    pub const O_DIRECTORY: u32 = 0x10000;
    pub const O_CLOEXEC: u32 = 0x80000;

    pub fn is_readable(&self) -> bool {
        let access = self.0 & 0x3;
        access == Self::O_RDONLY || access == Self::O_RDWR
    }

    pub fn is_writable(&self) -> bool {
        let access = self.0 & 0x3;
        access == Self::O_WRONLY || access == Self::O_RDWR
    }

    pub fn is_append(&self) -> bool {
        self.0 & Self::O_APPEND != 0
    }

    pub fn is_create(&self) -> bool {
        self.0 & Self::O_CREAT != 0
    }

    pub fn is_truncate(&self) -> bool {
        self.0 & Self::O_TRUNC != 0
    }
}

/// Seek origin (Linux-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SeekFrom {
    Start = 0,   // SEEK_SET
    Current = 1, // SEEK_CUR
    End = 2,     // SEEK_END
}

/// Type of file backing a file descriptor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Regular file on VFS
    Regular,
    /// Directory
    Directory,
    /// Character device (e.g., /dev/null, /dev/tty)
    CharDevice,
    /// Block device
    BlockDevice,
    /// Pipe (anonymous)
    Pipe,
    /// Named pipe (FIFO)
    Fifo,
    /// Socket
    Socket,
    /// Symbolic link
    Symlink,
    /// Standard I/O (stdin/stdout/stderr)
    StdIO,
    /// /proc pseudo-file
    ProcFile,
}

/// An open file description (shared between dup'd fds)
pub struct OpenFile {
    /// Path in the VFS
    pub path: String,
    /// File type
    pub file_type: FileType,
    /// Open flags
    pub flags: OpenFlags,
    /// Current file offset
    pub offset: usize,
    /// File size (cached)
    pub size: usize,
    /// Reference count (for dup/fork)
    pub ref_count: u32,
    /// Inode number (in VFS)
    pub inode: u64,
    /// Data buffer for pipe/device
    pub buffer: Option<Vec<u8>>,
    /// Close-on-exec flag
    pub cloexec: bool,
    /// Non-blocking flag
    pub nonblock: bool,
}

impl OpenFile {
    pub fn new(path: &str, file_type: FileType, flags: OpenFlags) -> Self {
        Self {
            path: String::from(path),
            file_type,
            flags,
            offset: 0,
            size: 0,
            ref_count: 1,
            inode: 0,
            buffer: None,
            cloexec: flags.0 & OpenFlags::O_CLOEXEC != 0,
            nonblock: flags.0 & OpenFlags::O_NONBLOCK != 0,
        }
    }

    pub fn new_stdio(fd: Fd) -> Self {
        let name = match fd {
            STDIN_FD => "/dev/stdin",
            STDOUT_FD => "/dev/stdout",
            STDERR_FD => "/dev/stderr",
            _ => "/dev/unknown",
        };
        Self {
            path: String::from(name),
            file_type: FileType::StdIO,
            flags: OpenFlags(if fd == STDIN_FD {
                OpenFlags::O_RDONLY
            } else {
                OpenFlags::O_WRONLY
            }),
            offset: 0,
            size: 0,
            ref_count: 1,
            inode: 0,
            buffer: None,
            cloexec: false,
            nonblock: false,
        }
    }
}

/// Per-process file descriptor table
pub struct FdTable {
    /// Map of fd number -> open file description
    pub files: BTreeMap<Fd, OpenFile>,
    /// Next fd to allocate
    pub next_fd: Fd,
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new()
    }
}

impl FdTable {
    /// Create a new fd table with standard I/O pre-opened
    pub fn new() -> Self {
        let mut table = Self {
            files: BTreeMap::new(),
            next_fd: 3,
        };
        // Pre-open stdin, stdout, stderr
        table.files.insert(STDIN_FD, OpenFile::new_stdio(STDIN_FD));
        table
            .files
            .insert(STDOUT_FD, OpenFile::new_stdio(STDOUT_FD));
        table
            .files
            .insert(STDERR_FD, OpenFile::new_stdio(STDERR_FD));
        table
    }

    /// Allocate the lowest available fd
    fn alloc_fd(&mut self) -> Option<Fd> {
        // Find lowest available fd starting from 0
        (0..MAX_FDS as Fd).find(|fd| !self.files.contains_key(fd))
    }

    /// Open a file and return its fd
    pub fn open(&mut self, path: &str, flags: OpenFlags, file_type: FileType) -> Result<Fd, i32> {
        let fd = self.alloc_fd().ok_or(-24i32)?; // EMFILE
        let file = OpenFile::new(path, file_type, flags);
        self.files.insert(fd, file);
        Ok(fd)
    }

    /// Close a file descriptor
    pub fn close(&mut self, fd: Fd) -> Result<(), i32> {
        if self.files.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(-9) // EBADF
        }
    }

    /// Get a reference to an open file
    pub fn get(&self, fd: Fd) -> Option<&OpenFile> {
        self.files.get(&fd)
    }

    /// Get a mutable reference to an open file
    pub fn get_mut(&mut self, fd: Fd) -> Option<&mut OpenFile> {
        self.files.get_mut(&fd)
    }

    /// Duplicate a file descriptor (dup)
    pub fn dup(&mut self, old_fd: Fd) -> Result<Fd, i32> {
        if !self.files.contains_key(&old_fd) {
            return Err(-9); // EBADF
        }
        let new_fd = self.alloc_fd().ok_or(-24i32)?; // EMFILE
        // Create a copy of the open file
        if let Some(old_file) = self.files.get(&old_fd) {
            let new_file = OpenFile {
                path: old_file.path.clone(),
                file_type: old_file.file_type,
                flags: old_file.flags,
                offset: old_file.offset,
                size: old_file.size,
                ref_count: 1,
                inode: old_file.inode,
                buffer: old_file.buffer.clone(),
                cloexec: false, // dup clears cloexec
                nonblock: old_file.nonblock,
            };
            self.files.insert(new_fd, new_file);
        }
        Ok(new_fd)
    }

    /// Duplicate to a specific fd number (dup2)
    pub fn dup2(&mut self, old_fd: Fd, new_fd: Fd) -> Result<Fd, i32> {
        if !self.files.contains_key(&old_fd) {
            return Err(-9); // EBADF
        }
        if old_fd == new_fd {
            return Ok(new_fd);
        }
        // Close new_fd if open
        let _ = self.close(new_fd);
        // Copy
        if let Some(old_file) = self.files.get(&old_fd) {
            let new_file = OpenFile {
                path: old_file.path.clone(),
                file_type: old_file.file_type,
                flags: old_file.flags,
                offset: old_file.offset,
                size: old_file.size,
                ref_count: 1,
                inode: old_file.inode,
                buffer: old_file.buffer.clone(),
                cloexec: false, // dup2 clears cloexec
                nonblock: old_file.nonblock,
            };
            self.files.insert(new_fd, new_file);
        }
        Ok(new_fd)
    }

    /// Read from a file descriptor
    pub fn read(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize, i32> {
        let file = self.files.get_mut(&fd).ok_or(-9i32)?; // EBADF

        if !file.flags.is_readable() && file.file_type != FileType::StdIO {
            return Err(-9); // EBADF
        }

        match file.file_type {
            FileType::StdIO => {
                // stdin - would block in real implementation
                Ok(0)
            }
            FileType::CharDevice => {
                match file.path.as_str() {
                    "/dev/null" => Ok(0), // Always EOF
                    "/dev/zero" => {
                        // Fill with zeros
                        for b in buf.iter_mut() {
                            *b = 0;
                        }
                        Ok(buf.len())
                    }
                    "/dev/random" | "/dev/urandom" => {
                        // Pseudo-random bytes (simple LCG)
                        static SEED: AtomicI32 = AtomicI32::new(12345);
                        for b in buf.iter_mut() {
                            let s = SEED.load(Ordering::Relaxed);
                            let next = s.wrapping_mul(1103515245).wrapping_add(12345);
                            SEED.store(next, Ordering::Relaxed);
                            *b = (next >> 16) as u8;
                        }
                        Ok(buf.len())
                    }
                    _ => Ok(0),
                }
            }
            FileType::Pipe => {
                if let Some(ref mut buffer) = file.buffer {
                    let to_read = buf.len().min(buffer.len());
                    buf[..to_read].copy_from_slice(&buffer[..to_read]);
                    buffer.drain(..to_read);
                    Ok(to_read)
                } else {
                    Ok(0)
                }
            }
            FileType::Regular | FileType::ProcFile => {
                // Read from VFS
                if let Some(data) = crate::vfs::VFS.lock().read_file(&file.path) {
                    let available = if file.offset >= data.len() {
                        0
                    } else {
                        data.len() - file.offset
                    };
                    let to_read = buf.len().min(available);
                    if to_read > 0 {
                        buf[..to_read].copy_from_slice(&data[file.offset..file.offset + to_read]);
                        file.offset += to_read;
                    }
                    Ok(to_read)
                } else {
                    Err(-2) // ENOENT
                }
            }
            _ => Err(-22), // EINVAL
        }
    }

    /// Write to a file descriptor
    pub fn write(&mut self, fd: Fd, buf: &[u8]) -> Result<usize, i32> {
        let file = self.files.get_mut(&fd).ok_or(-9i32)?; // EBADF

        match file.file_type {
            FileType::StdIO => {
                // stdout/stderr -> serial output
                if fd == STDOUT_FD || fd == STDERR_FD {
                    for &byte in buf {
                        crate::serial_print!("{}", byte as char);
                    }
                    Ok(buf.len())
                } else {
                    Err(-9) // EBADF (stdin is not writable)
                }
            }
            FileType::CharDevice => {
                match file.path.as_str() {
                    "/dev/null" => Ok(buf.len()), // Discard
                    "/dev/tty" | "/dev/console" => {
                        for &byte in buf {
                            crate::serial_print!("{}", byte as char);
                        }
                        Ok(buf.len())
                    }
                    _ => Err(-22), // EINVAL
                }
            }
            FileType::Pipe => {
                if file.buffer.is_none() {
                    file.buffer = Some(Vec::new());
                }
                if let Some(ref mut buffer) = file.buffer {
                    buffer.extend_from_slice(buf);
                    Ok(buf.len())
                } else {
                    Err(-5) // EIO
                }
            }
            FileType::Regular => {
                // Write to VFS
                let data = if file.flags.is_append() {
                    // Append mode
                    let mut existing = crate::vfs::VFS
                        .lock()
                        .read_file(&file.path)
                        .unwrap_or_default()
                        .to_vec();
                    existing.extend_from_slice(buf);
                    existing
                } else {
                    // Overwrite from offset
                    let mut existing = crate::vfs::VFS
                        .lock()
                        .read_file(&file.path)
                        .unwrap_or_default()
                        .to_vec();
                    // Extend if needed
                    while existing.len() < file.offset + buf.len() {
                        existing.push(0);
                    }
                    existing[file.offset..file.offset + buf.len()].copy_from_slice(buf);
                    file.offset += buf.len();
                    existing
                };
                crate::vfs::VFS.lock().write_file(&file.path, &data);
                Ok(buf.len())
            }
            _ => Err(-22), // EINVAL
        }
    }

    /// Seek to a position in a file
    pub fn lseek(&mut self, fd: Fd, offset: i64, whence: SeekFrom) -> Result<usize, i32> {
        let file = self.files.get_mut(&fd).ok_or(-9i32)?; // EBADF

        match file.file_type {
            FileType::Pipe | FileType::Socket => return Err(-29), // ESPIPE
            _ => {}
        }

        let new_offset = match whence {
            SeekFrom::Start => offset as usize,
            SeekFrom::Current => {
                let current = file.offset as i64;
                (current + offset) as usize
            }
            SeekFrom::End => {
                let size = file.size as i64;
                (size + offset) as usize
            }
        };

        file.offset = new_offset;
        Ok(new_offset)
    }

    /// Get file status
    pub fn fstat(&self, fd: Fd) -> Result<FileStat, i32> {
        let file = self.files.get(&fd).ok_or(-9i32)?; // EBADF
        Ok(FileStat {
            st_dev: 0,
            st_ino: file.inode,
            st_mode: match file.file_type {
                FileType::Regular => 0o100644,
                FileType::Directory => 0o040755,
                FileType::CharDevice => 0o020666,
                FileType::BlockDevice => 0o060660,
                FileType::Pipe | FileType::Fifo => 0o010644,
                FileType::Socket => 0o140755,
                FileType::Symlink => 0o120777,
                FileType::StdIO => 0o020666,
                FileType::ProcFile => 0o100444,
            },
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            st_size: file.size as u64,
            st_blksize: 4096,
            st_blocks: (file.size.div_ceil(512)) as u64,
            st_atime: 0,
            st_mtime: 0,
            st_ctime: 0,
        })
    }

    /// Count open file descriptors
    pub fn count(&self) -> usize {
        self.files.len()
    }

    /// Set close-on-exec flag
    pub fn set_cloexec(&mut self, fd: Fd, cloexec: bool) {
        if let Some(file) = self.files.get_mut(&fd) {
            file.cloexec = cloexec;
        }
    }

    /// Get close-on-exec flag
    pub fn get_cloexec(&self, fd: Fd) -> bool {
        self.files.get(&fd).map(|f| f.cloexec).unwrap_or(false)
    }

    /// Set non-blocking flag
    pub fn set_nonblock(&mut self, fd: Fd, nonblock: bool) {
        if let Some(file) = self.files.get_mut(&fd) {
            file.nonblock = nonblock;
        }
    }

    /// Get non-blocking flag
    pub fn get_nonblock(&self, fd: Fd) -> bool {
        self.files.get(&fd).map(|f| f.nonblock).unwrap_or(false)
    }

    /// Get raw open flags
    pub fn get_open_flags(&self, fd: Fd) -> u32 {
        self.files.get(&fd).map(|f| f.flags.0).unwrap_or(0)
    }

    /// List all open fds
    pub fn list_fds(&self) -> Vec<Fd> {
        self.files.keys().cloned().collect()
    }
}

/// File status structure (Linux stat-compatible)
#[derive(Debug, Clone)]
pub struct FileStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u64,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: u64,
    pub st_blksize: u64,
    pub st_blocks: u64,
    pub st_atime: u64,
    pub st_mtime: u64,
    pub st_ctime: u64,
}

/// Global FD tables - one per process
/// In a real implementation this would be in the process struct
lazy_static::lazy_static! {
    pub static ref PROCESS_FD_TABLES: Mutex<BTreeMap<u32, FdTable>> = {
        let mut tables = BTreeMap::new();
        // Create default fd tables for init processes
        tables.insert(0, FdTable::new()); // kernel
        tables.insert(1, FdTable::new()); // init
        tables.insert(2, FdTable::new()); // knoxos-desktop
        Mutex::new(tables)
    };
}

/// Get or create fd table for a process
pub fn get_fd_table(pid: u32) -> &'static Mutex<BTreeMap<u32, FdTable>> {
    &PROCESS_FD_TABLES
}

/// Create fd table for a new process
pub fn create_fd_table(pid: u32) {
    PROCESS_FD_TABLES.lock().insert(pid, FdTable::new());
}

/// Remove fd table when process exits
pub fn destroy_fd_table(pid: u32) {
    PROCESS_FD_TABLES.lock().remove(&pid);
}

// ═══════════════════════════════════════════════════════════════════════════
// PER-PROCESS CONVENIENCE WRAPPERS (4.2)
// These functions determine the current PID and operate on its fd table.
// ═══════════════════════════════════════════════════════════════════════════

/// Get the current process's PID (with fallback to PID 0)
fn current_pid() -> u32 {
    crate::scheduler::current_pid().unwrap_or(0)
}

/// Open a file in the current process's fd table
pub fn sys_open(path: &str, flags: OpenFlags, file_type: FileType) -> Result<Fd, i32> {
    let pid = current_pid();
    let mut tables = PROCESS_FD_TABLES.lock();
    let table = tables.entry(pid).or_default();
    table.open(path, flags, file_type)
}

/// Close a file descriptor in the current process's fd table
pub fn sys_close(fd: Fd) -> Result<(), i32> {
    let pid = current_pid();
    let mut tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get_mut(&pid) {
        table.close(fd)
    } else {
        Err(-9) // EBADF
    }
}

/// Read from a file descriptor in the current process's fd table
pub fn sys_read(fd: Fd, buf: &mut [u8]) -> Result<usize, i32> {
    let pid = current_pid();
    let mut tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get_mut(&pid) {
        table.read(fd, buf)
    } else {
        Err(-9) // EBADF
    }
}

/// Write to a file descriptor in the current process's fd table
pub fn sys_write(fd: Fd, buf: &[u8]) -> Result<usize, i32> {
    let pid = current_pid();
    let mut tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get_mut(&pid) {
        table.write(fd, buf)
    } else {
        Err(-9) // EBADF
    }
}

/// Seek in a file descriptor in the current process's fd table
pub fn sys_lseek(fd: Fd, offset: i64, whence: u32) -> Result<u64, i32> {
    let pid = current_pid();
    let seek_from = match whence {
        0 => SeekFrom::Start,
        1 => SeekFrom::Current,
        2 => SeekFrom::End,
        _ => return Err(-22), // EINVAL
    };
    let mut tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get_mut(&pid) {
        table.lseek(fd, offset, seek_from).map(|o| o as u64)
    } else {
        Err(-9) // EBADF
    }
}

/// Stat a file descriptor in the current process's fd table
pub fn sys_fstat(fd: Fd) -> Result<FileStat, i32> {
    let pid = current_pid();
    let tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get(&pid) {
        table.fstat(fd)
    } else {
        Err(-9) // EBADF
    }
}

/// Get the path of an open file descriptor
pub fn sys_fd_path(fd: Fd) -> Option<String> {
    let pid = current_pid();
    let tables = PROCESS_FD_TABLES.lock();
    tables
        .get(&pid)
        .and_then(|t| t.files.get(&fd).map(|f| f.path.clone()))
}

/// Fork fd table from parent to child (called during fork())
/// Duplicates all open file descriptors from parent to child process.
/// File descriptions (OpenFile) have their ref_count incremented.
pub fn fork_fd_table(parent_pid: u32, child_pid: u32) {
    let mut tables = PROCESS_FD_TABLES.lock();

    // Clone the parent's fd table
    let child_table = if let Some(parent_table) = tables.get(&parent_pid) {
        let mut new_table = FdTable::new();
        new_table.next_fd = parent_table.next_fd;
        // Clone all open files (shallow clone — in a real OS we'd share file descriptions)
        for (&fd, open_file) in &parent_table.files {
            let cloned = OpenFile {
                path: open_file.path.clone(),
                file_type: open_file.file_type,
                flags: open_file.flags,
                offset: open_file.offset,
                size: open_file.size,
                ref_count: 1, // New reference for child
                inode: open_file.inode,
                buffer: open_file.buffer.clone(),
                cloexec: open_file.cloexec,
                nonblock: open_file.nonblock,
            };
            new_table.files.insert(fd, cloned);
        }
        new_table
    } else {
        FdTable::new()
    };

    tables.insert(child_pid, child_table);
    serial_println!(
        "[fd] fork_fd_table: PID {} -> PID {}",
        parent_pid,
        child_pid
    );
}

/// Duplicate a file descriptor (module-level wrapper)
pub fn dup(oldfd: usize) -> Result<usize, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get_mut(&pid) {
        table.dup(oldfd as Fd).map(|fd| fd as usize)
    } else {
        Err(-9) // EBADF
    }
}

/// Duplicate to a specific fd number (module-level wrapper)
pub fn dup2(oldfd: usize, newfd: usize) -> Result<usize, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get_mut(&pid) {
        table.dup2(oldfd as Fd, newfd as Fd).map(|fd| fd as usize)
    } else {
        Err(-9) // EBADF
    }
}

/// Close all file descriptors with O_CLOEXEC flag (called during exec())
pub fn close_cloexec_fds(pid: u32) {
    let mut tables = PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get_mut(&pid) {
        let cloexec_fds: Vec<Fd> = table
            .files
            .iter()
            .filter(|(_, f)| f.cloexec || f.flags.0 & OpenFlags::O_CLOEXEC != 0)
            .map(|(&fd, _)| fd)
            .collect();
        let count = cloexec_fds.len();
        for fd in cloexec_fds {
            table.files.remove(&fd);
        }
        if count > 0 {
            serial_println!("[fd] close_cloexec: PID {} closed {} fds", pid, count);
        }
    }
}
