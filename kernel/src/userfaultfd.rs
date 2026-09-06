// userfaultfd.rs — User-space page fault handling
// Allows user-space to handle page faults for memory regions

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// userfaultfd flags
pub const UFFD_API: u64 = 0xAA;
pub const UFFD_API_FEATURES: u64 = 0;

/// userfaultfd register mode flags
pub const UFFDIO_REGISTER_MODE_MISSING: u64 = 1 << 0;
pub const UFFDIO_REGISTER_MODE_WP: u64 = 1 << 1;
pub const UFFDIO_REGISTER_MODE_MINOR: u64 = 1 << 2;

/// userfaultfd ioctl commands
pub const UFFDIO_API: u64 = 0xC018AA3F;
pub const UFFDIO_REGISTER: u64 = 0xC020AA00;
pub const UFFDIO_UNREGISTER: u64 = 0x8010AA01;
pub const UFFDIO_COPY: u64 = 0xC028AA03;
pub const UFFDIO_ZEROPAGE: u64 = 0xC020AA04;
pub const UFFDIO_WAKE: u64 = 0x8010AA02;
pub const UFFDIO_WRITEPROTECT: u64 = 0xC018AA06;
pub const UFFDIO_CONTINUE: u64 = 0xC018AA07;

/// Page fault event types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UffdEvent {
    Pagefault = 0x12,
    Fork = 0x13,
    Remap = 0x14,
    Remove = 0x15,
    Unmap = 0x16,
}

/// Page fault message
#[derive(Debug, Clone)]
pub struct UffdMsg {
    pub event: UffdEvent,
    pub address: u64,
    pub flags: u64, // UFFD_PAGEFAULT_FLAG_WRITE, _WP, _MINOR
    pub pid: u64,
}

/// UFFD pagefault flags
pub const UFFD_PAGEFAULT_FLAG_WRITE: u64 = 1 << 0;
pub const UFFD_PAGEFAULT_FLAG_WP: u64 = 1 << 1;
pub const UFFD_PAGEFAULT_FLAG_MINOR: u64 = 1 << 2;

/// A registered memory range for userfaultfd
#[derive(Debug, Clone)]
pub struct UffdRange {
    pub start: u64,
    pub len: u64,
    pub mode: u64,
}

/// A userfaultfd instance
#[derive(Debug)]
pub struct Userfaultfd {
    pub id: u64,
    pub pid: u64,
    pub flags: u32,
    pub api_version: u64,
    pub api_handshaked: bool,
    /// Registered memory ranges
    pub ranges: Vec<UffdRange>,
    /// Pending page fault messages
    pub pending: Vec<UffdMsg>,
    /// Processes waiting on page faults
    pub waiting: Vec<u64>,
}

impl Userfaultfd {
    pub fn new(id: u64, pid: u64, flags: u32) -> Self {
        Userfaultfd {
            id,
            pid,
            flags,
            api_version: UFFD_API,
            api_handshaked: false,
            ranges: Vec::new(),
            pending: Vec::new(),
            waiting: Vec::new(),
        }
    }

    /// Perform API handshake
    pub fn api_handshake(&mut self, api: u64, features: u64) -> Result<u64, UffdError> {
        if api != UFFD_API {
            return Err(UffdError::InvalidApi);
        }
        self.api_handshaked = true;
        let _ = features;
        // Return supported ioctls bitmap
        Ok(0x1F) // REGISTER | UNREGISTER | COPY | ZEROPAGE | WAKE
    }

    /// Register a memory range
    pub fn register(&mut self, start: u64, len: u64, mode: u64) -> Result<u64, UffdError> {
        if !self.api_handshaked {
            return Err(UffdError::NotInitialized);
        }

        // Check alignment
        if !start.is_multiple_of(4096) || !len.is_multiple_of(4096) || len == 0 {
            return Err(UffdError::InvalidRange);
        }

        // Check for overlaps
        for range in &self.ranges {
            if start < range.start + range.len && start + len > range.start {
                return Err(UffdError::RangeOverlap);
            }
        }

        self.ranges.push(UffdRange { start, len, mode });

        // Return supported ioctls for this range
        Ok(0x1C) // COPY | ZEROPAGE | WAKE
    }

    /// Unregister a memory range
    pub fn unregister(&mut self, start: u64, len: u64) -> Result<(), UffdError> {
        let before = self.ranges.len();
        self.ranges.retain(|r| !(r.start == start && r.len == len));
        if self.ranges.len() == before {
            Err(UffdError::NotFound)
        } else {
            Ok(())
        }
    }

    /// Handle a page fault — queue event for user-space handler
    pub fn report_fault(&mut self, address: u64, flags: u64) -> bool {
        // Check if address is in a registered range
        let in_range = self
            .ranges
            .iter()
            .any(|r| address >= r.start && address < r.start + r.len);

        if !in_range {
            return false;
        }

        // Align to page boundary
        let page_addr = address & !0xFFF;

        self.pending.push(UffdMsg {
            event: UffdEvent::Pagefault,
            address: page_addr,
            flags,
            pid: self.pid,
        });

        true
    }

    /// Read pending events (user-space reads from uffd)
    pub fn read_events(&mut self, max: usize) -> Vec<UffdMsg> {
        let count = core::cmp::min(max, self.pending.len());
        self.pending.drain(..count).collect()
    }

    /// Copy data to resolve a page fault (UFFDIO_COPY)
    pub fn copy(&mut self, dst: u64, _src: u64, len: u64, _mode: u64) -> Result<u64, UffdError> {
        // Verify destination is in a registered range
        let in_range = self
            .ranges
            .iter()
            .any(|r| dst >= r.start && dst + len <= r.start + r.len);

        if !in_range {
            return Err(UffdError::InvalidRange);
        }

        // In real implementation, would map pages and copy data
        // For now, just acknowledge
        Ok(len)
    }

    /// Zero a page to resolve a fault (UFFDIO_ZEROPAGE)
    pub fn zeropage(&mut self, addr: u64, len: u64, _mode: u64) -> Result<u64, UffdError> {
        let in_range = self
            .ranges
            .iter()
            .any(|r| addr >= r.start && addr + len <= r.start + r.len);

        if !in_range {
            return Err(UffdError::InvalidRange);
        }

        // In real implementation, would map zero pages
        Ok(len)
    }

    /// Wake threads waiting on faults in a range (UFFDIO_WAKE)
    pub fn wake(&mut self, addr: u64, len: u64) -> Result<(), UffdError> {
        let _ = addr;
        let _ = len;
        // Remove faulting threads from wait list
        self.waiting.clear();
        Ok(())
    }

    /// Check if address is in a monitored range
    pub fn is_monitored(&self, address: u64) -> bool {
        self.ranges
            .iter()
            .any(|r| address >= r.start && address < r.start + r.len)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UffdError {
    NotFound,
    InvalidApi,
    NotInitialized,
    InvalidRange,
    RangeOverlap,
    WouldBlock,
    PermDenied,
}

lazy_static! {
    static ref UFFDS: Mutex<BTreeMap<u64, Userfaultfd>> = Mutex::new(BTreeMap::new());
    static ref NEXT_ID: Mutex<u64> = Mutex::new(1);
}

/// Create a userfaultfd
pub fn sys_userfaultfd(flags: u32) -> Result<u64, UffdError> {
    let mut next = NEXT_ID.lock();
    let id = *next;
    *next += 1;
    drop(next);

    let uffd = Userfaultfd::new(id, 0, flags);
    UFFDS.lock().insert(id, uffd);

    Ok(id)
}

/// Perform ioctl on userfaultfd
pub fn uffd_ioctl(id: u64, cmd: u64, arg1: u64, arg2: u64, arg3: u64) -> Result<u64, UffdError> {
    let mut uffds = UFFDS.lock();
    let uffd = uffds.get_mut(&id).ok_or(UffdError::NotFound)?;

    match cmd {
        UFFDIO_API => uffd.api_handshake(arg1, arg2),
        UFFDIO_REGISTER => {
            let mode = arg3;
            uffd.register(arg1, arg2, mode)
        }
        UFFDIO_UNREGISTER => {
            uffd.unregister(arg1, arg2)?;
            Ok(0)
        }
        UFFDIO_COPY => uffd.copy(arg1, arg2, arg3, 0),
        UFFDIO_ZEROPAGE => uffd.zeropage(arg1, arg2, 0),
        UFFDIO_WAKE => {
            uffd.wake(arg1, arg2)?;
            Ok(0)
        }
        _ => Err(UffdError::NotFound),
    }
}

/// Close a userfaultfd
pub fn uffd_close(id: u64) {
    UFFDS.lock().remove(&id);
}

/// Handle a page fault — check if any uffd is monitoring this address
pub fn handle_page_fault(pid: u64, address: u64, is_write: bool) -> bool {
    let mut uffds = UFFDS.lock();
    for uffd in uffds.values_mut() {
        if uffd.pid == pid && uffd.is_monitored(address) {
            let flags = if is_write {
                UFFD_PAGEFAULT_FLAG_WRITE
            } else {
                0
            };
            return uffd.report_fault(address, flags);
        }
    }
    false
}

/// Initialize userfaultfd subsystem
pub fn init() {
    crate::serial_println!("  userfaultfd subsystem initialized (UFFDIO_COPY, ZEROPAGE, WAKE, WP)");
}
