// SPDX-License-Identifier: MIT
//! Multiboot2 / Limine boot protocol support (item 1.8)
//!
//! Parses Multiboot2 and Limine boot information structures so KnoxOS
//! can be loaded by GRUB2, Limine, or any Multiboot2-compliant bootloader.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

// ── Multiboot2 tag types ─────────────────────────────────────────────
const MB2_TAG_END: u32 = 0;
const MB2_TAG_CMDLINE: u32 = 1;
const MB2_TAG_BOOT_LOADER: u32 = 2;
const MB2_TAG_MODULE: u32 = 3;
const MB2_TAG_BASIC_MEMINFO: u32 = 4;
const MB2_TAG_BOOTDEV: u32 = 5;
const MB2_TAG_MMAP: u32 = 6;
const MB2_TAG_FRAMEBUFFER: u32 = 8;
const MB2_TAG_ELF_SECTIONS: u32 = 9;
const MB2_TAG_APM: u32 = 10;
const MB2_TAG_ACPI_OLD: u32 = 14;
const MB2_TAG_ACPI_NEW: u32 = 15;

// ── Multiboot2 Header ────────────────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Multiboot2Header {
    pub magic: u32,
    pub architecture: u32,
    pub header_length: u32,
    pub checksum: u32,
}

// ── Multiboot2 Tag Header ────────────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TagHeader {
    pub tag_type: u32,
    pub size: u32,
}

// ── Memory map entry ─────────────────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MmapEntry {
    pub base_addr: u64,
    pub length: u64,
    pub entry_type: u32,
    pub reserved: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    Available,
    Reserved,
    AcpiReclaimable,
    AcpiNvs,
    BadMemory,
    Unknown(u32),
}

impl From<u32> for MemoryRegionType {
    fn from(t: u32) -> Self {
        match t {
            1 => MemoryRegionType::Available,
            2 => MemoryRegionType::Reserved,
            3 => MemoryRegionType::AcpiReclaimable,
            4 => MemoryRegionType::AcpiNvs,
            5 => MemoryRegionType::BadMemory,
            x => MemoryRegionType::Unknown(x),
        }
    }
}

// ── Framebuffer info ─────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct FramebufferInfo {
    pub addr: u64,
    pub pitch: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub fb_type: u8,
}

// ── Parsed Multiboot2 info ───────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Multiboot2Info {
    pub command_line: Option<String>,
    pub bootloader_name: Option<String>,
    pub memory_map: Vec<(u64, u64, MemoryRegionType)>,
    pub framebuffer: Option<FramebufferInfo>,
    pub mem_lower: u32,
    pub mem_upper: u32,
    pub rsdp_v1_addr: Option<u64>,
    pub rsdp_v2_addr: Option<u64>,
}

// ── Limine protocol structures ───────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LimineRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: u64,
}

#[derive(Debug, Clone)]
pub struct LimineInfo {
    pub command_line: Option<String>,
    pub memory_map: Vec<(u64, u64, MemoryRegionType)>,
    pub framebuffer: Option<FramebufferInfo>,
    pub rsdp_addr: Option<u64>,
    pub kernel_addr: u64,
    pub hhdm_offset: u64,
}

// ── Boot protocol detection ──────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootProtocol {
    BootloaderApi,
    Multiboot2,
    Limine,
    Unknown,
}

lazy_static::lazy_static! {
    static ref BOOT_INFO: Mutex<Option<BootInfoUnified>> = Mutex::new(None);
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static TOTAL_MEMORY: AtomicU64 = AtomicU64::new(0);

/// Unified boot info across all protocols
#[derive(Debug, Clone)]
pub struct BootInfoUnified {
    pub protocol: BootProtocol,
    pub command_line: Option<String>,
    pub memory_map: Vec<(u64, u64, MemoryRegionType)>,
    pub framebuffer: Option<FramebufferInfo>,
    pub total_memory: u64,
    pub rsdp_addr: Option<u64>,
}

/// Parse Multiboot2 boot information from a pointer
pub fn parse_multiboot2(info_addr: u64) -> Option<Multiboot2Info> {
    let ptr = info_addr as *const u8;
    // Safety: caller must ensure this points to valid Multiboot2 info
    let total_size = unsafe { *(ptr as *const u32) } as usize;

    let mut info = Multiboot2Info {
        command_line: None,
        bootloader_name: None,
        memory_map: Vec::new(),
        framebuffer: None,
        mem_lower: 0,
        mem_upper: 0,
        rsdp_v1_addr: None,
        rsdp_v2_addr: None,
    };

    let mut offset = 8usize; // skip total_size + reserved
    while offset < total_size {
        let tag = unsafe { &*(ptr.add(offset) as *const TagHeader) };
        if tag.tag_type == MB2_TAG_END {
            break;
        }

        match tag.tag_type {
            MB2_TAG_CMDLINE => {
                let str_ptr = unsafe { ptr.add(offset + 8) };
                let len = tag.size as usize - 9; // minus header + null
                let slice = unsafe { core::slice::from_raw_parts(str_ptr, len) };
                if let Ok(s) = core::str::from_utf8(slice) {
                    info.command_line = Some(String::from(s));
                }
            }
            MB2_TAG_BOOT_LOADER => {
                let str_ptr = unsafe { ptr.add(offset + 8) };
                let len = tag.size as usize - 9;
                let slice = unsafe { core::slice::from_raw_parts(str_ptr, len) };
                if let Ok(s) = core::str::from_utf8(slice) {
                    info.bootloader_name = Some(String::from(s));
                }
            }
            MB2_TAG_BASIC_MEMINFO => {
                let data = unsafe { ptr.add(offset + 8) as *const u32 };
                info.mem_lower = unsafe { *data };
                info.mem_upper = unsafe { *data.add(1) };
            }
            MB2_TAG_MMAP => {
                let entry_size = unsafe { *(ptr.add(offset + 8) as *const u32) } as usize;
                let _entry_ver = unsafe { *(ptr.add(offset + 12) as *const u32) };
                let entries_start = offset + 16;
                let entries_end = offset + tag.size as usize;
                let mut eoff = entries_start;
                while eoff + entry_size <= entries_end {
                    let entry = unsafe { &*(ptr.add(eoff) as *const MmapEntry) };
                    info.memory_map.push((
                        entry.base_addr,
                        entry.length,
                        MemoryRegionType::from(entry.entry_type),
                    ));
                    eoff += entry_size;
                }
            }
            MB2_TAG_FRAMEBUFFER => {
                let fb_addr = unsafe { *(ptr.add(offset + 8) as *const u64) };
                let fb_pitch = unsafe { *(ptr.add(offset + 16) as *const u32) };
                let fb_width = unsafe { *(ptr.add(offset + 20) as *const u32) };
                let fb_height = unsafe { *(ptr.add(offset + 24) as *const u32) };
                let fb_bpp = unsafe { *ptr.add(offset + 28) };
                let fb_type = unsafe { *ptr.add(offset + 29) };
                info.framebuffer = Some(FramebufferInfo {
                    addr: fb_addr,
                    pitch: fb_pitch,
                    width: fb_width,
                    height: fb_height,
                    bpp: fb_bpp,
                    fb_type,
                });
            }
            MB2_TAG_ACPI_OLD => {
                info.rsdp_v1_addr = Some(info_addr + offset as u64 + 8);
            }
            MB2_TAG_ACPI_NEW => {
                info.rsdp_v2_addr = Some(info_addr + offset as u64 + 8);
            }
            _ => {} // skip unknown tags
        }

        // Tags are 8-byte aligned
        offset += ((tag.size as usize) + 7) & !7;
    }

    Some(info)
}

/// Calculate total available memory from memory map
pub fn total_available_memory(map: &[(u64, u64, MemoryRegionType)]) -> u64 {
    map.iter()
        .filter(|(_, _, t)| *t == MemoryRegionType::Available)
        .map(|(_, len, _)| *len)
        .sum()
}

/// Initialize boot info from Multiboot2 protocol
pub fn init_from_multiboot2(info_addr: u64) {
    if let Some(mb2) = parse_multiboot2(info_addr) {
        let total_mem = total_available_memory(&mb2.memory_map);
        let unified = BootInfoUnified {
            protocol: BootProtocol::Multiboot2,
            command_line: mb2.command_line,
            memory_map: mb2.memory_map,
            framebuffer: mb2.framebuffer,
            total_memory: total_mem,
            rsdp_addr: mb2.rsdp_v2_addr.or(mb2.rsdp_v1_addr),
        };
        TOTAL_MEMORY.store(total_mem, Ordering::Relaxed);
        *BOOT_INFO.lock() = Some(unified);
        INITIALIZED.store(true, Ordering::Release);
        crate::serial_println!(
            "[multiboot] Multiboot2 info parsed, {}MiB RAM",
            total_mem >> 20
        );
    }
}

/// Initialize using the default bootloader_api path
pub fn init_from_bootloader_api(total_memory: u64) {
    let unified = BootInfoUnified {
        protocol: BootProtocol::BootloaderApi,
        command_line: None,
        memory_map: Vec::new(),
        framebuffer: None,
        total_memory,
        rsdp_addr: None,
    };
    TOTAL_MEMORY.store(total_memory, Ordering::Relaxed);
    *BOOT_INFO.lock() = Some(unified);
    INITIALIZED.store(true, Ordering::Release);
    crate::serial_println!(
        "[multiboot] bootloader_api path, {}MiB RAM",
        total_memory >> 20
    );
}

/// Get the boot protocol that was used
pub fn boot_protocol() -> BootProtocol {
    BOOT_INFO
        .lock()
        .as_ref()
        .map_or(BootProtocol::Unknown, |i| i.protocol)
}

/// Get the kernel command line
pub fn command_line() -> Option<String> {
    BOOT_INFO
        .lock()
        .as_ref()
        .and_then(|i| i.command_line.clone())
}

/// Get total available memory in bytes
pub fn total_memory() -> u64 {
    TOTAL_MEMORY.load(Ordering::Relaxed)
}

/// Get RSDP address if available from boot protocol
pub fn rsdp_addr() -> Option<u64> {
    BOOT_INFO.lock().as_ref().and_then(|i| i.rsdp_addr)
}

/// Initialize the multiboot module
pub fn init() {
    // Default: assume bootloader_api path was used
    if !INITIALIZED.load(Ordering::Acquire) {
        init_from_bootloader_api(0);
    }
    crate::serial_println!("[multiboot] init complete, protocol={:?}", boot_protocol());
}
