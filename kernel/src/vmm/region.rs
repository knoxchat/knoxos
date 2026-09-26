//! Mapping types, protection flags, and virtual memory areas.
use alloc::string::String;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::paging::PageTableFlags;
#[cfg(target_arch = "x86_64")]
use x86_64::structures::paging::PageTableFlags;

use super::layout::PAGE_SIZE;

/// Type of memory mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingType {
    /// Program text/code from ELF
    Text,
    /// Program data from ELF (.data, .bss)
    Data,
    /// Heap (brk/sbrk)
    Heap,
    /// Stack
    Stack,
    /// Guard page (unmapped, triggers fault on access)
    Guard,
    /// Anonymous mmap
    Anonymous,
    /// File-backed mmap
    FileBacked,
    /// Shared memory
    Shared,
    /// Kernel mapping (not user-accessible)
    Kernel,
}

/// Protection flags for a memory region
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtFlags {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl ProtFlags {
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const RWX: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
    pub const R: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
    pub const NONE: Self = Self {
        read: false,
        write: false,
        execute: false,
    };

    /// Convert to x86_64 page table flags
    pub fn to_page_flags(self) -> PageTableFlags {
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if self.write {
            flags |= PageTableFlags::WRITABLE;
        }
        if !self.execute {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        flags
    }

    /// Create from Linux mmap prot constants
    pub fn from_mmap_prot(prot: u64) -> Self {
        const PROT_READ: u64 = 0x1;
        const PROT_WRITE: u64 = 0x2;
        const PROT_EXEC: u64 = 0x4;
        Self {
            read: prot & PROT_READ != 0,
            write: prot & PROT_WRITE != 0,
            execute: prot & PROT_EXEC != 0,
        }
    }
}

/// Flags for mmap
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmapFlags {
    pub shared: bool,
    pub anonymous: bool,
    pub fixed: bool,
    pub populate: bool,
}

impl MmapFlags {
    pub fn from_linux(flags: u64) -> Self {
        const MAP_SHARED: u64 = 0x01;
        const MAP_PRIVATE: u64 = 0x02;
        const MAP_FIXED: u64 = 0x10;
        const MAP_ANONYMOUS: u64 = 0x20;
        const MAP_POPULATE: u64 = 0x08000;
        Self {
            shared: flags & MAP_SHARED != 0,
            anonymous: flags & MAP_ANONYMOUS != 0,
            fixed: flags & MAP_FIXED != 0,
            populate: flags & MAP_POPULATE != 0,
        }
    }
}

/// A virtual memory area (VMA) — contiguous region with uniform protections
#[derive(Debug, Clone)]
pub struct VirtualMemoryArea {
    /// Start virtual address (page-aligned)
    pub start: u64,
    /// End virtual address (exclusive, page-aligned)
    pub end: u64,
    /// Protection flags
    pub prot: ProtFlags,
    /// Mapping type
    pub mapping_type: MappingType,
    /// Mapping flags
    pub flags: MmapFlags,
    /// File path for file-backed mappings
    pub file_path: Option<String>,
    /// File offset for file-backed mappings
    pub file_offset: u64,
    /// Whether pages are copy-on-write
    pub cow: bool,
    /// Reference count for shared mappings
    pub ref_count: u64,
}

impl VirtualMemoryArea {
    /// Number of pages in this VMA
    pub fn page_count(&self) -> u64 {
        (self.end - self.start) / PAGE_SIZE
    }

    /// Check if an address falls within this VMA
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }
}
