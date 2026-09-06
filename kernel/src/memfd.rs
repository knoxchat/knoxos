// memfd.rs — memfd_create() implementation
// Anonymous file-backed memory regions with fd-based access
// Supports MFD_CLOEXEC, MFD_ALLOW_SEALING, file seals

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// memfd_create flags
pub const MFD_CLOEXEC: u32 = 0x0001;
pub const MFD_ALLOW_SEALING: u32 = 0x0002;
pub const MFD_HUGETLB: u32 = 0x0004;

/// File seal flags (for fcntl F_ADD_SEALS / F_GET_SEALS)
pub const F_SEAL_SEAL: u32 = 0x0001; // Prevent further seals
pub const F_SEAL_SHRINK: u32 = 0x0002; // Prevent file shrink
pub const F_SEAL_GROW: u32 = 0x0004; // Prevent file grow
pub const F_SEAL_WRITE: u32 = 0x0008; // Prevent writes
pub const F_SEAL_FUTURE_WRITE: u32 = 0x0010; // Prevent future mmap writes

/// fcntl commands for seals
pub const F_ADD_SEALS: u32 = 1033;
pub const F_GET_SEALS: u32 = 1034;

/// A memfd instance
#[derive(Debug)]
pub struct MemFd {
    pub id: u64,
    pub name: String,
    pub data: Vec<u8>,
    pub flags: u32,
    pub seals: u32,
    pub created_pid: u64,
    pub ref_count: u32,
}

impl MemFd {
    pub fn new(id: u64, name: &str, flags: u32) -> Self {
        MemFd {
            id,
            name: String::from(name),
            data: Vec::new(),
            flags,
            seals: 0,
            created_pid: 0,
            ref_count: 1,
        }
    }

    /// Write data at offset
    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, MemFdError> {
        if self.seals & F_SEAL_WRITE != 0 {
            return Err(MemFdError::Sealed);
        }

        let end = offset + data.len();
        if end > self.data.len() {
            if self.seals & F_SEAL_GROW != 0 {
                return Err(MemFdError::Sealed);
            }
            self.data.resize(end, 0);
        }

        self.data[offset..end].copy_from_slice(data);
        Ok(data.len())
    }

    /// Read data from offset
    pub fn read(&self, offset: usize, buf: &mut [u8]) -> usize {
        if offset >= self.data.len() {
            return 0;
        }
        let available = self.data.len() - offset;
        let to_read = core::cmp::min(buf.len(), available);
        buf[..to_read].copy_from_slice(&self.data[offset..offset + to_read]);
        to_read
    }

    /// Truncate / resize
    pub fn truncate(&mut self, size: usize) -> Result<(), MemFdError> {
        if size < self.data.len() && self.seals & F_SEAL_SHRINK != 0 {
            return Err(MemFdError::Sealed);
        }
        if size > self.data.len() && self.seals & F_SEAL_GROW != 0 {
            return Err(MemFdError::Sealed);
        }
        self.data.resize(size, 0);
        Ok(())
    }

    /// Add seals
    pub fn add_seals(&mut self, new_seals: u32) -> Result<u32, MemFdError> {
        if self.flags & MFD_ALLOW_SEALING == 0 {
            return Err(MemFdError::SealingNotAllowed);
        }
        if self.seals & F_SEAL_SEAL != 0 {
            return Err(MemFdError::Sealed);
        }
        self.seals |= new_seals;
        Ok(self.seals)
    }

    /// Get current seals
    pub fn get_seals(&self) -> u32 {
        self.seals
    }

    /// Get file size
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemFdError {
    NotFound,
    Sealed,
    SealingNotAllowed,
    InvalidName,
    TooMany,
}

lazy_static! {
    static ref MEMFDS: Mutex<MemFdTable> = Mutex::new(MemFdTable::new());
}

struct MemFdTable {
    fds: BTreeMap<u64, MemFd>,
    next_id: u64,
}

impl MemFdTable {
    fn new() -> Self {
        MemFdTable {
            fds: BTreeMap::new(),
            next_id: 1,
        }
    }
}

/// memfd_create syscall — create anonymous memory-backed file
pub fn sys_memfd_create(name: &str, flags: u32) -> Result<u64, MemFdError> {
    // Validate name (max 249 chars, no slashes)
    if name.len() > 249 || name.contains('/') {
        return Err(MemFdError::InvalidName);
    }

    let mut table = MEMFDS.lock();
    let id = table.next_id;
    table.next_id += 1;

    let memfd = MemFd::new(id, name, flags);
    table.fds.insert(id, memfd);

    Ok(id)
}

/// Write to memfd
pub fn memfd_write(id: u64, offset: usize, data: &[u8]) -> Result<usize, MemFdError> {
    let mut table = MEMFDS.lock();
    let fd = table.fds.get_mut(&id).ok_or(MemFdError::NotFound)?;
    fd.write(offset, data)
}

/// Read from memfd
pub fn memfd_read(id: u64, offset: usize, buf: &mut [u8]) -> Result<usize, MemFdError> {
    let table = MEMFDS.lock();
    let fd = table.fds.get(&id).ok_or(MemFdError::NotFound)?;
    Ok(fd.read(offset, buf))
}

/// Truncate / ftruncate memfd
pub fn memfd_truncate(id: u64, size: usize) -> Result<(), MemFdError> {
    let mut table = MEMFDS.lock();
    let fd = table.fds.get_mut(&id).ok_or(MemFdError::NotFound)?;
    fd.truncate(size)
}

/// Add seals (fcntl F_ADD_SEALS)
pub fn memfd_add_seals(id: u64, seals: u32) -> Result<u32, MemFdError> {
    let mut table = MEMFDS.lock();
    let fd = table.fds.get_mut(&id).ok_or(MemFdError::NotFound)?;
    fd.add_seals(seals)
}

/// Get seals (fcntl F_GET_SEALS)
pub fn memfd_get_seals(id: u64) -> Result<u32, MemFdError> {
    let table = MEMFDS.lock();
    let fd = table.fds.get(&id).ok_or(MemFdError::NotFound)?;
    Ok(fd.get_seals())
}

/// Get memfd size
pub fn memfd_size(id: u64) -> Result<usize, MemFdError> {
    let table = MEMFDS.lock();
    let fd = table.fds.get(&id).ok_or(MemFdError::NotFound)?;
    Ok(fd.size())
}

/// Close/release memfd
pub fn memfd_close(id: u64) {
    let mut table = MEMFDS.lock();
    if let Some(fd) = table.fds.get_mut(&id) {
        if fd.ref_count > 1 {
            fd.ref_count -= 1;
        } else {
            table.fds.remove(&id);
        }
    }
}

/// Duplicate memfd reference (for dup/fork)
pub fn memfd_dup(id: u64) -> Result<u64, MemFdError> {
    let mut table = MEMFDS.lock();
    let fd = table.fds.get_mut(&id).ok_or(MemFdError::NotFound)?;
    fd.ref_count += 1;
    Ok(id)
}

/// Get memfd info for /proc/[pid]/fdinfo
pub fn memfd_info(id: u64) -> Option<MemFdInfo> {
    let table = MEMFDS.lock();
    table.fds.get(&id).map(|fd| MemFdInfo {
        id: fd.id,
        name: fd.name.clone(),
        size: fd.data.len(),
        seals: fd.seals,
        flags: fd.flags,
        ref_count: fd.ref_count,
    })
}

#[derive(Debug, Clone)]
pub struct MemFdInfo {
    pub id: u64,
    pub name: String,
    pub size: usize,
    pub seals: u32,
    pub flags: u32,
    pub ref_count: u32,
}

/// Initialize memfd subsystem
pub fn init() {
    crate::serial_println!(
        "  memfd subsystem initialized (MFD_CLOEXEC, MFD_ALLOW_SEALING, file seals)"
    );
}
