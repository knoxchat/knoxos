#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::VirtAddr;
#[cfg(target_arch = "x86_64")]
use crate::arch_compat::structures::paging::VirtAddr;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
};
use crate::serial_println;
/// ELF Binary Loader - Loads and executes ELF64 Linux binaries
/// Supports static ELF64 executables for x86_64
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(target_arch = "x86_64")]
use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB};

/// ELF magic number
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class
const ELFCLASS64: u8 = 2;

/// ELF data encoding
const ELFDATA2LSB: u8 = 1; // Little endian

/// ELF type
const ET_EXEC: u16 = 2; // Executable
const ET_DYN: u16 = 3; // Shared object (PIE)

/// ELF machine type
const EM_X86_64: u16 = 62;

/// Program header type
const PT_LOAD: u32 = 1; // Loadable segment
const PT_INTERP: u32 = 3; // Interpreter path
const PT_NOTE: u32 = 4; // Note segment
const PT_PHDR: u32 = 6; // Program header table

/// Program header flags
const PF_X: u32 = 1; // Execute
const PF_W: u32 = 2; // Write
const PF_R: u32 = 4; // Read

/// ELF64 file header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64, // Entry point virtual address
    pub e_phoff: u64, // Program header table offset
    pub e_shoff: u64, // Section header table offset
    pub e_flags: u32,
    pub e_ehsize: u16,    // ELF header size
    pub e_phentsize: u16, // Program header entry size
    pub e_phnum: u16,     // Number of program headers
    pub e_shentsize: u16, // Section header entry size
    pub e_shnum: u16,     // Number of section headers
    pub e_shstrndx: u16,  // Section name string table index
}

/// ELF64 program header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64, // Offset in file
    pub p_vaddr: u64,  // Virtual address in memory
    pub p_paddr: u64,  // Physical address (ignored)
    pub p_filesz: u64, // Size in file
    pub p_memsz: u64,  // Size in memory (may be larger than filesz for .bss)
    pub p_align: u64,  // Alignment
}

/// ELF64 section header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64SectionHeader {
    pub sh_name: u32,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_link: u32,
    pub sh_info: u32,
    pub sh_addralign: u64,
    pub sh_entsize: u64,
}

/// Result of loading an ELF binary
#[derive(Debug)]
pub struct ElfLoadResult {
    /// Entry point address
    pub entry_point: u64,
    /// Program break (end of loaded segments)
    pub brk: u64,
    /// Whether the binary is position-independent
    pub is_pie: bool,
    /// Loaded segments info
    pub segments: Vec<LoadedSegment>,
    /// Interpreter path (for dynamically linked binaries)
    pub interpreter: Option<String>,
}

/// A loaded memory segment
#[derive(Debug, Clone)]
pub struct LoadedSegment {
    pub vaddr: u64,
    pub memsz: u64,
    pub flags: u32,
}

/// ELF loading error
#[derive(Debug)]
pub enum ElfError {
    InvalidMagic,
    InvalidClass,
    InvalidEndian,
    InvalidMachine,
    InvalidType,
    TooSmall,
    LoadFailed(String),
}

impl core::fmt::Display for ElfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ElfError::InvalidMagic => write!(f, "Invalid ELF magic number"),
            ElfError::InvalidClass => write!(f, "Not ELF64"),
            ElfError::InvalidEndian => write!(f, "Not little-endian"),
            ElfError::InvalidMachine => write!(f, "Not x86_64"),
            ElfError::InvalidType => write!(f, "Not executable"),
            ElfError::TooSmall => write!(f, "File too small"),
            ElfError::LoadFailed(msg) => write!(f, "Load failed: {}", msg),
        }
    }
}

/// Validate an ELF binary
pub fn validate_elf(data: &[u8]) -> Result<&Elf64Header, ElfError> {
    if data.len() < core::mem::size_of::<Elf64Header>() {
        return Err(ElfError::TooSmall);
    }

    // Check magic
    if data[0..4] != ELF_MAGIC {
        return Err(ElfError::InvalidMagic);
    }

    let header = unsafe { &*(data.as_ptr() as *const Elf64Header) };

    // Check class (64-bit)
    if header.e_ident[4] != ELFCLASS64 {
        return Err(ElfError::InvalidClass);
    }

    // Check endianness
    if header.e_ident[5] != ELFDATA2LSB {
        return Err(ElfError::InvalidEndian);
    }

    // Check machine
    if header.e_machine != EM_X86_64 {
        return Err(ElfError::InvalidMachine);
    }

    // Check type
    if header.e_type != ET_EXEC && header.e_type != ET_DYN {
        return Err(ElfError::InvalidType);
    }

    Ok(header)
}

/// Parse program headers from ELF data
pub fn parse_program_headers(data: &[u8], header: &Elf64Header) -> Vec<Elf64ProgramHeader> {
    let mut phdrs = Vec::new();
    let ph_offset = header.e_phoff as usize;
    let ph_size = header.e_phentsize as usize;
    let ph_count = header.e_phnum as usize;

    for i in 0..ph_count {
        let offset = ph_offset + i * ph_size;
        if offset + ph_size <= data.len() {
            let phdr = unsafe { *(data.as_ptr().add(offset) as *const Elf64ProgramHeader) };
            phdrs.push(phdr);
        }
    }

    phdrs
}

/// Load an ELF binary into memory (preparation phase)
/// In a real implementation, this would map pages into the process address space
pub fn load_elf(data: &[u8]) -> Result<ElfLoadResult, ElfError> {
    let header = validate_elf(data)?;

    serial_println!(
        "[KnoxOS] Loading ELF: entry={:#x}, type={}",
        header.e_entry,
        if header.e_type == ET_EXEC {
            "EXEC"
        } else {
            "DYN"
        }
    );

    let phdrs = parse_program_headers(data, header);
    let mut segments = Vec::new();
    let mut max_addr: u64 = 0;
    let mut interpreter: Option<String> = None;

    for phdr in &phdrs {
        match phdr.p_type {
            PT_LOAD => {
                serial_println!(
                    "[KnoxOS]   LOAD: vaddr={:#x} memsz={:#x} filesz={:#x} flags={:#x}",
                    phdr.p_vaddr,
                    phdr.p_memsz,
                    phdr.p_filesz,
                    phdr.p_flags
                );

                let end = phdr.p_vaddr + phdr.p_memsz;
                if end > max_addr {
                    max_addr = end;
                }

                segments.push(LoadedSegment {
                    vaddr: phdr.p_vaddr,
                    memsz: phdr.p_memsz,
                    flags: phdr.p_flags,
                });
            }
            PT_INTERP => {
                // Extract interpreter path
                let start = phdr.p_offset as usize;
                let end = start + phdr.p_filesz as usize;
                if end <= data.len() {
                    let path_bytes = &data[start..end];
                    if let Some(null_pos) = path_bytes.iter().position(|&b| b == 0) {
                        if let Ok(path) = core::str::from_utf8(&path_bytes[..null_pos]) {
                            interpreter = Some(String::from(path));
                            serial_println!("[KnoxOS]   INTERP: {}", path);
                        }
                    }
                }
            }
            PT_NOTE => {
                serial_println!("[KnoxOS]   NOTE segment");
            }
            PT_PHDR => {
                serial_println!("[KnoxOS]   PHDR: vaddr={:#x}", phdr.p_vaddr);
            }
            _ => {}
        }
    }

    // Align brk to page boundary
    let brk = (max_addr + 0xFFF) & !0xFFF;

    Ok(ElfLoadResult {
        entry_point: header.e_entry,
        brk,
        is_pie: header.e_type == ET_DYN,
        segments,
        interpreter,
    })
}

/// Check if data looks like an ELF binary
/// Execute an ELF binary at the given path with arguments
pub fn exec_elf_path(path: &str, args: &[&str]) {
    if let Some(data) = crate::vfs::read_file_dispatch(path) {
        match load_elf(&data) {
            Ok(result) => {
                crate::serial_println!("[ELF] Loaded {}: entry={:#x}", path, result.entry_point);
            }
            Err(e) => {
                crate::serial_println!("[ELF] Failed to load {}: {:?}", path, e);
            }
        }
    } else {
        crate::serial_println!("[ELF] File not found: {}", path);
    }
}

pub fn is_elf(data: &[u8]) -> bool {
    data.len() >= 4 && data[0..4] == ELF_MAGIC
}

/// Map ELF segments into a process address space
///
/// # Safety
/// Caller must ensure the mapper and frame allocator are valid, and that
/// the virtual address range is available for mapping.
pub unsafe fn map_elf_segments<M, A>(
    data: &[u8],
    mapper: &mut M,
    frame_allocator: &mut A,
) -> Result<ElfLoadResult, ElfError>
where
    M: Mapper<Size4KiB>,
    A: FrameAllocator<Size4KiB>,
{
    let header = validate_elf(data)?;
    let phdrs = parse_program_headers(data, header);
    let mut segments = Vec::new();
    let mut max_addr: u64 = 0;
    let mut interpreter: Option<String> = None;

    for phdr in &phdrs {
        if phdr.p_type != PT_LOAD {
            if phdr.p_type == PT_INTERP {
                let start = phdr.p_offset as usize;
                let end = start + phdr.p_filesz as usize;
                if end <= data.len() {
                    let path_bytes = &data[start..end];
                    if let Some(null_pos) = path_bytes.iter().position(|&b| b == 0) {
                        if let Ok(path) = core::str::from_utf8(&path_bytes[..null_pos]) {
                            interpreter = Some(String::from(path));
                        }
                    }
                }
            }
            continue;
        }

        // Convert ELF flags to page table flags
        let mut page_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if phdr.p_flags & PF_W != 0 {
            page_flags |= PageTableFlags::WRITABLE;
        }
        if phdr.p_flags & PF_X == 0 {
            page_flags |= PageTableFlags::NO_EXECUTE;
        }

        // Map pages for this segment
        let start_page = phdr.p_vaddr & !0xFFF; // Page-align down
        let end_addr = phdr.p_vaddr + phdr.p_memsz;
        let end_page = (end_addr + 0xFFF) & !0xFFF; // Page-align up

        serial_println!(
            "[KnoxOS] ELF map: vaddr={:#x}-{:#x} flags={:?}",
            start_page,
            end_page,
            page_flags
        );

        let mut vaddr = start_page;
        while vaddr < end_page {
            let page = Page::<Size4KiB>::containing_address(VirtAddr::new(vaddr));
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(ElfError::LoadFailed(String::from("Out of physical frames")))?;

            mapper
                .map_to(page, frame, page_flags, frame_allocator)
                .map_err(|_| ElfError::LoadFailed(String::from("Page mapping failed")))?
                .flush();

            // Copy file data into the mapped page
            let page_start = vaddr;
            let page_end = vaddr + 4096;
            let seg_file_start = phdr.p_vaddr;
            let seg_file_end = phdr.p_vaddr + phdr.p_filesz;

            // Zero the page first (handles .bss)
            let page_ptr = vaddr as *mut u8;
            core::ptr::write_bytes(page_ptr, 0, 4096);

            // Calculate overlap between this page and the file data region
            let copy_start = page_start.max(seg_file_start);
            let copy_end = page_end.min(seg_file_end);

            if copy_start < copy_end {
                let file_offset = phdr.p_offset + (copy_start - phdr.p_vaddr);
                let dest_offset = copy_start - page_start;
                let copy_len = (copy_end - copy_start) as usize;

                if (file_offset as usize + copy_len) <= data.len() {
                    let src = &data[file_offset as usize..file_offset as usize + copy_len];
                    let dest = core::slice::from_raw_parts_mut(
                        (page_start + dest_offset) as *mut u8,
                        copy_len,
                    );
                    dest.copy_from_slice(src);
                }
            }

            vaddr += 4096;
        }

        let end = phdr.p_vaddr + phdr.p_memsz;
        if end > max_addr {
            max_addr = end;
        }

        segments.push(LoadedSegment {
            vaddr: phdr.p_vaddr,
            memsz: phdr.p_memsz,
            flags: phdr.p_flags,
        });
    }

    let brk = (max_addr + 0xFFF) & !0xFFF;

    Ok(ElfLoadResult {
        entry_point: header.e_entry,
        brk,
        is_pie: header.e_type == ET_DYN,
        segments,
        interpreter,
    })
}

/// Allocate and map a user-mode stack
///
/// # Safety
/// Caller must ensure mapper and frame allocator are valid.
pub unsafe fn map_user_stack<M, A>(
    mapper: &mut M,
    frame_allocator: &mut A,
    stack_top: u64,
    stack_pages: usize,
) -> Result<u64, ElfError>
where
    M: Mapper<Size4KiB>,
    A: FrameAllocator<Size4KiB>,
{
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::NO_EXECUTE;

    let stack_bottom = stack_top - (stack_pages as u64 * 4096);

    for i in 0..stack_pages {
        let addr = stack_bottom + (i as u64 * 4096);
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(ElfError::LoadFailed(String::from(
                "Out of frames for stack",
            )))?;

        mapper
            .map_to(page, frame, flags, frame_allocator)
            .map_err(|_| ElfError::LoadFailed(String::from("Stack mapping failed")))?
            .flush();

        // Zero the stack page
        core::ptr::write_bytes(addr as *mut u8, 0, 4096);
    }

    serial_println!(
        "[KnoxOS] User stack mapped: {:#x}-{:#x} ({} pages)",
        stack_bottom,
        stack_top,
        stack_pages
    );

    Ok(stack_top)
}

/// Get ELF binary info as a string (for debugging)
pub fn elf_info(data: &[u8]) -> Option<String> {
    let header = validate_elf(data).ok()?;
    let phdrs = parse_program_headers(data, header);

    let load_segments = phdrs.iter().filter(|p| p.p_type == PT_LOAD).count();

    Some(alloc::format!(
        "ELF64 {} x86_64, entry={:#x}, {} program headers, {} LOAD segments",
        if header.e_type == ET_EXEC {
            "executable"
        } else {
            "shared object"
        },
        header.e_entry,
        header.e_phnum,
        load_segments,
    ))
}

/// Initialize ELF loader
pub fn init() {
    serial_println!("[KnoxOS] ELF64 loader initialized (x86_64 Linux ABI)");
}
